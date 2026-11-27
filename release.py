#!/usr/bin/env python3

import argparse
import getpass
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

PROJECT_NAME = "yatws"
ASSETS_DIR = Path("release_artifacts")
PQC_PUBKEY_FILENAME = f"{PROJECT_NAME}-pqc-pubkey.asc"
# gpg-agent cache id used by the `sq-pqc` wrapper (~/emacs/sq-pqc.sh: CACHE_ID).
PQC_AGENT_CACHE_ID = "sq-pqc"
PQC_UNLOCK_ATTEMPTS = 3


def run_cmd(cmd, shell=False, cwd=None, dry_run=False, cleanup_local_tag=None):
    """Executes a CLI command and halts on failure (rolling back an unpushed local tag if requested)."""
    rendered = " ".join(cmd) if isinstance(cmd, list) else cmd
    if dry_run:
        print(f"[dry-run] Would run: {rendered}")
        return
    print(f"[*] Running: {rendered}")
    try:
        subprocess.run(cmd, shell=shell, check=True, cwd=cwd)
    except (subprocess.CalledProcessError, KeyboardInterrupt) as e:
        reason = "interrupted" if isinstance(e, KeyboardInterrupt) else str(e)
        print(f"\n[ERROR] Command failed: {reason}")
        if cleanup_local_tag:
            print(f"[*] Rolling back unpushed local tag '{cleanup_local_tag}'...")
            subprocess.run(["git", "tag", "-d", cleanup_local_tag], check=False)
        sys.exit(1)


def ensure_workspace_root():
    """Switches to the repository root when invoked via `bazel run`."""
    workspace_dir = os.environ.get("BUILD_WORKSPACE_DIRECTORY")
    if workspace_dir:
        os.chdir(workspace_dir)


def ensure_local_bins_in_path():
    """Ensures ~/.local/bin and ~/.cargo/bin are on PATH under `bazel run`."""
    home = Path.home()
    extra_dirs = [str(home / ".local" / "bin"), str(home / ".cargo" / "bin")]
    current_parts = os.environ.get("PATH", "").split(os.pathsep)
    missing = [d for d in extra_dirs if os.path.isdir(d) and d not in current_parts]
    if missing:
        os.environ["PATH"] = os.pathsep.join(missing + current_parts)


def configure_gpg_tty():
    """Ensures pinentry/password prompts find the controlling terminal under `bazel run`."""
    if os.environ.get("GPG_TTY"):
        return
    for fd in (0, 1, 2):
        try:
            if os.isatty(fd):
                os.environ["GPG_TTY"] = os.ttyname(fd)
                return
        except OSError:
            continue


def git_config_get(key):
    res = subprocess.run(
        ["git", "config", "--get", key],
        capture_output=True,
        text=True,
        check=False,
    )
    val = res.stdout.strip()
    return val if res.returncode == 0 and val else None


def resolve_commit_sha(ref):
    res = subprocess.run(
        ["git", "rev-parse", f"{ref}^{{commit}}"],
        capture_output=True,
        text=True,
        check=True,
    )
    return res.stdout.strip()


def resolve_pqc_key(sq_bin, cli_pqc_key):
    """Resolves `(fingerprint, algo, error_msg)` for the OpenPGP v6 PQC key in `sq-pqc`."""
    export_cmd = [sq_bin, "cert", "export"]
    if cli_pqc_key:
        export_cmd += ["--cert", cli_pqc_key]
    else:
        email = git_config_get("user.email")
        if not email:
            return None, None, "`git config user.email` is unset and `--pqc-key` was not provided"
        export_cmd += ["--cert-email", email]

    exported = subprocess.run(export_cmd, capture_output=True, check=False)
    if exported.returncode != 0 or not exported.stdout:
        target = cli_pqc_key or git_config_get("user.email")
        return None, None, f"no certificate for '{target}' in `{sq_bin}` cert store"

    inspected = subprocess.run(
        [sq_bin, "inspect"],
        input=exported.stdout,
        capture_output=True,
        check=False,
    )
    text = inspected.stdout.decode("utf-8", errors="replace")

    candidates = []
    for block in text.split("OpenPGP Certificate.")[1:]:
        fpr_match = re.search(r"Fingerprint:\s*([0-9A-Fa-f]{40,64})", block)
        algo_match = re.search(r"Public-key algo:\s*(.+)", block)
        if fpr_match:
            fpr = fpr_match.group(1).upper()
            algo = algo_match.group(1).strip() if algo_match else "unknown"
            candidates.append((fpr, algo))

    if not candidates:
        return None, None, f"unable to parse certificate fingerprint from `{sq_bin} inspect`"

    pqc_candidates = [
        (fpr, algo)
        for fpr, algo in candidates
        if len(fpr) == 64 or "ML-DSA" in algo or "SLH-DSA" in algo
    ]
    selected_fpr, selected_algo = pqc_candidates[0] if pqc_candidates else candidates[0]

    key_list = subprocess.run(
        [sq_bin, "key", "list"],
        capture_output=True,
        text=True,
        check=False,
    )
    if key_list.returncode != 0 or selected_fpr not in key_list.stdout.upper():
        return None, None, f"secret key for {selected_fpr} not found in `{sq_bin} key list`"

    return selected_fpr, selected_algo, None


def _gpg_agent_request(commands, passphrase=None):
    """Runs Assuan `commands` on gpg-agent; returns the final OK/ERR line (None if unreachable).

    A `PASSPHRASE` inquiry (loopback pinentry) is answered with `passphrase`; any other
    inquiry is cancelled, so this never pops up a pinentry.
    """
    if shutil.which("gpgconf") is None:
        return None
    sock_path = subprocess.run(
        ["gpgconf", "--list-dirs", "agent-socket"], capture_output=True, text=True, check=False
    ).stdout.strip()
    if not sock_path:
        return None
    subprocess.run(["gpg-connect-agent", "/bye"], capture_output=True, check=False)  # autostart

    def escape(data):
        return "".join(
            f"%{b:02X}" if b < 0x20 or b == ord("%") else chr(b) for b in data.encode("utf-8")
        )

    try:
        with socket.socket(socket.AF_UNIX) as sock:
            sock.settimeout(30)
            sock.connect(sock_path)
            stream = sock.makefile("rwb")

            def read_response():
                while True:
                    line = stream.readline().decode("utf-8", errors="replace").rstrip("\n")
                    if not line:
                        return "ERR connection closed"
                    if line.startswith("INQUIRE "):
                        if line.split()[1] == "PASSPHRASE" and passphrase is not None:
                            stream.write(f"D {escape(passphrase)}\nEND\n".encode("utf-8"))
                        else:
                            stream.write(b"CAN\n")
                        stream.flush()
                    elif line.startswith(("OK", "ERR")):
                        return line

            last = read_response()
            for cmd in commands:
                if not last.startswith("OK"):
                    break
                stream.write(f"{cmd}\n".encode("utf-8"))
                stream.flush()
                last = read_response()
            return last
    except OSError:
        return None


def pqc_password_cached():
    """True/False if gpg-agent does/doesn't hold the `sq-pqc` password; None if no agent."""
    res = _gpg_agent_request([f"GET_PASSPHRASE --no-ask {PQC_AGENT_CACHE_ID} X X X"])
    return None if res is None else res.startswith("OK")


def seed_pqc_password(passphrase):
    """Stores `passphrase` in gpg-agent's cache via loopback pinentry (no curses/GUI dialog)."""
    res = _gpg_agent_request(
        [
            "OPTION pinentry-mode=loopback",
            f"GET_PASSPHRASE {PQC_AGENT_CACHE_ID} X Passphrase X",
        ],
        passphrase=passphrase,
    )
    return res is not None and res.startswith("OK")


def unlock_pqc_key(sq_bin, fpr):
    """Unlocks the PQC key once, up front, with a plain prompt instead of pinentry.

    The `sq-pqc` wrapper fetches the password from gpg-agent, whose curses pinentry
    garbles terminals like Emacs `M-x shell`. If the password isn't cached, prompt
    with `getpass` (comint hides the input), seed gpg-agent's cache through loopback
    mode, then prove it with a throwaway signature so a typo is caught before tagging.
    """
    for attempt in range(1, PQC_UNLOCK_ATTEMPTS + 1):
        cached = pqc_password_cached()
        if cached is False:
            try:
                pw = getpass.getpass(f"    Passphrase for sq-pqc key {fpr[:16]}...: ")
            except (EOFError, KeyboardInterrupt):
                print()
                return False, "passphrase entry aborted"
            seeded = seed_pqc_password(pw)
            del pw
            if not seeded:
                return False, "could not cache passphrase in gpg-agent (allow-loopback-pinentry?)"

        test = subprocess.run(
            [sq_bin, "sign", "--message", "--signer", fpr],
            input=b"yatws release preflight\n",
            capture_output=True,
            check=False,
        )
        if test.returncode == 0:
            return True, "unlocked (test signature OK)"

        err = test.stderr.decode("utf-8", errors="replace").strip().splitlines()
        err_line = err[-1].strip() if err else f"exit {test.returncode}"
        if cached is None or attempt == PQC_UNLOCK_ATTEMPTS:
            return False, f"test signature failed ({err_line})"
        # The wrapper clears the cached password on a failed unlock, so we re-prompt.
        print(f"    Unlock failed ({err_line}); try again.")
    return False, "unlock failed"


def read_current_version():
    """Reads the current crate version from yatws/Cargo.toml."""
    cargo_toml = Path(PROJECT_NAME) / "Cargo.toml"
    with open(cargo_toml, "rb") as f:
        manifest = tomllib.load(f)
    version = manifest.get("package", {}).get("version")
    if not version:
        print(f"[ERROR] Could not read [package].version from {cargo_toml}")
        sys.exit(1)
    return version


def compute_next_version(current_version, target=None):
    """Computes the next version from `current_version` and optional `target`.

    If `target` is None or 'patch', increments the last numeric component (e.g. 0.1.9 -> 0.1.10).
    Supports 'minor', 'major', or an explicit version string (with or without leading 'v').
    """
    if target is None or target == "patch":
        base = current_version.split("-", 1)[0].split("+", 1)[0]
        parts = base.split(".")
        if not parts or not all(p.isdigit() for p in parts):
            print(f"[ERROR] Cannot auto-bump non-numeric version '{current_version}'; pass an explicit version.")
            sys.exit(1)
        parts[-1] = str(int(parts[-1]) + 1)
        return ".".join(parts)

    if target in ("minor", "major"):
        base = current_version.split("-", 1)[0].split("+", 1)[0]
        parts = base.split(".")
        if len(parts) < 3 or not all(p.isdigit() for p in parts[:3]):
            print(f"[ERROR] Cannot apply '{target}' bump to '{current_version}'; pass an explicit version.")
            sys.exit(1)
        major, minor, _patch = int(parts[0]), int(parts[1]), int(parts[2])
        if target == "minor":
            return f"{major}.{minor + 1}.0"
        return f"{major + 1}.0.0"

    cleaned = target.lstrip("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", cleaned):
        print(f"[ERROR] Invalid target version '{target}' (expected e.g. 0.1.10, patch, minor, or major).")
        sys.exit(1)
    return cleaned


def update_cargo_toml_version(cargo_toml_path, new_version):
    """Updates `[package].version` in Cargo.toml in place, preserving formatting."""
    lines = cargo_toml_path.read_text(encoding="utf-8").splitlines(keepends=True)
    in_package = False
    replaced = False
    for idx, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("[") and not stripped.startswith("[["):
            in_package = stripped == "[package]"
            continue
        if in_package:
            m = re.match(r'^(\s*version\s*=\s*")([^"]+)(".*)$', line, re.DOTALL)
            if m:
                lines[idx] = f"{m.group(1)}{new_version}{m.group(3)}"
                replaced = True
                break
    if not replaced:
        print(f"[ERROR] Could not find [package].version in {cargo_toml_path}")
        sys.exit(1)
    cargo_toml_path.write_text("".join(lines), encoding="utf-8")


def update_module_bazel_version(module_bazel_path, new_version):
    """Updates `module(..., version = "...")` in MODULE.bazel in place."""
    content = module_bazel_path.read_text(encoding="utf-8")
    new_content, count = re.subn(
        r'(module\s*\([^)]*?version\s*=\s*")([^"]+)(")',
        lambda m: f"{m.group(1)}{new_version}{m.group(3)}",
        content,
        count=1,
        flags=re.DOTALL,
    )
    if count != 1:
        print(f"[ERROR] Could not find module(..., version = \"...\") in {module_bazel_path}")
        sys.exit(1)
    module_bazel_path.write_text(new_content, encoding="utf-8")


def bump_version(target=None, dry_run=False):
    """Bumps version across Cargo.toml, MODULE.bazel, Cargo.lock, and llms.txt in the working tree."""
    current_version = read_current_version()
    new_version = compute_next_version(current_version, target)

    if new_version == current_version:
        print(f"[ERROR] Target version {new_version} is already the current version.")
        sys.exit(1)

    cargo_toml = Path(PROJECT_NAME) / "Cargo.toml"
    module_bazel = Path("MODULE.bazel")

    print(f"=== Bumping {PROJECT_NAME} version: {current_version} -> {new_version} ===")

    missing_tools = [t for t in ("cargo", "bazel") if shutil.which(t) is None]
    if missing_tools:
        print(f"[ERROR] Missing required CLI tool(s) for version bump: {', '.join(missing_tools)}")
        sys.exit(1)

    if dry_run:
        print(f"[dry-run] Would update {cargo_toml}: {current_version} -> {new_version}")
        if module_bazel.exists():
            print(f"[dry-run] Would update {module_bazel}: {current_version} -> {new_version}")
        print("[dry-run] Would run: cargo update --workspace --offline")
        print("[dry-run] Would run: bazel run //yatws:gen_llms_txt")
        return new_version

    update_cargo_toml_version(cargo_toml, new_version)
    print(f"[*] Updated {cargo_toml} -> {new_version}")

    if module_bazel.exists():
        update_module_bazel_version(module_bazel, new_version)
        print(f"[*] Updated {module_bazel} -> {new_version}")

    run_cmd(["cargo", "update", "--workspace", "--offline"])
    run_cmd(["bazel", "run", "//yatws:gen_llms_txt"])

    mismatches = check_version_consistency(new_version)
    if mismatches:
        print(f"[ERROR] Version consistency check failed after bump: {', '.join(mismatches)}")
        sys.exit(1)

    print(f"\n=== Bumped {PROJECT_NAME} to {new_version} ===")
    print("Review with `git status` / `git diff`, commit & push, then run:")
    print(f"  bazel run //:release -- {new_version}")
    return new_version


def check_version_consistency(version):
    """Ensures Cargo.toml, Cargo.lock, MODULE.bazel, and llms.txt all match `version`."""
    mismatches = []

    cargo_toml = Path(PROJECT_NAME) / "Cargo.toml"
    with open(cargo_toml, "rb") as f:
        manifest = tomllib.load(f)
    crate_ver = manifest.get("package", {}).get("version")
    if crate_ver != version:
        mismatches.append(f"{cargo_toml}={crate_ver}")

    cargo_lock = Path("Cargo.lock")
    if cargo_lock.exists():
        with open(cargo_lock, "rb") as f:
            lock_data = tomllib.load(f)
        lock_vers = [
            pkg.get("version")
            for pkg in lock_data.get("package", [])
            if pkg.get("name") == PROJECT_NAME
        ]
        if lock_vers and lock_vers[0] != version:
            mismatches.append(f"Cargo.lock={lock_vers[0]}")

    module_bazel = Path("MODULE.bazel")
    if module_bazel.exists():
        content = module_bazel.read_text(encoding="utf-8")
        m = re.search(r'module\s*\([^)]*?version\s*=\s*"([^"]+)"', content, re.DOTALL)
        if m and m.group(1) != version:
            mismatches.append(f"MODULE.bazel={m.group(1)}")

    llms_txt = Path(PROJECT_NAME) / "llms.txt"
    if llms_txt.exists():
        head = llms_txt.read_text(encoding="utf-8")[:512]
        m = re.search(r"\*\*Version:\*\*\s*(\S+)", head)
        if m and m.group(1) != version:
            mismatches.append(
                f"{llms_txt}={m.group(1)} (run `bazel run //yatws:gen_llms_txt`)"
            )

    return mismatches


def load_crates_io_token():
    """Loads the crates.io token from env or ~/.cargo/credentials[.toml] without printing it."""
    for env_var in ("CARGO_REGISTRIES_CRATES_IO_TOKEN", "CARGO_REGISTRY_TOKEN"):
        val = os.environ.get(env_var, "").strip()
        if val:
            return val

    for cred_name in ("credentials.toml", "credentials"):
        cred_path = Path.home() / ".cargo" / cred_name
        if cred_path.exists():
            try:
                with open(cred_path, "rb") as f:
                    data = tomllib.load(f)
                token = (
                    data.get("registries", {}).get("crates-io", {}).get("token")
                    or data.get("registry", {}).get("token")
                )
                if token and token.strip():
                    return token.strip()
            except Exception:
                continue
    return None


def verify_crates_io_token(token):
    """Verifies the crates.io API token is active and not expired/revoked."""
    req = urllib.request.Request(
        "https://crates.io/api/v1/me",
        headers={
            "User-Agent": "yatws-release-preflight/0.1",
            "Accept": "application/json",
            "Authorization": token,
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            body = json.loads(resp.read().decode("utf-8"))
            login = body.get("user", {}).get("login", "authenticated")
            return True, f"token valid ({login})"
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", errors="replace")
        try:
            details = "; ".join(
                err.get("detail", "") for err in json.loads(raw).get("errors", [])
            )
        except Exception:
            details = raw.strip()
        lower = details.lower()
        # Scoped tokens pass token validation in crates.io middleware first; `/api/v1/me`
        # then returns a website-only notice rather than "authentication failed".
        if e.code == 403 and "crates.io website" in lower and "authentication failed" not in lower:
            return True, "scoped token valid"
        return (
            False,
            f"crates.io rejected token (HTTP {e.code}: {details or 'authentication failed'}) "
            "— create a token with `publish-update` scope at https://crates.io/settings/tokens and run `cargo login`",
        )
    except Exception as e:
        return False, f"could not verify token against crates.io: {e}"


def check_crates_io(version):
    """Checks crates.io token validity and ensures `version` is not already published."""
    token = load_crates_io_token()
    if not token:
        return False, "no crates.io token found (run `cargo login`)"

    token_ok, token_msg = verify_crates_io_token(token)
    if not token_ok:
        return False, token_msg

    url = f"https://index.crates.io/ya/tw/{PROJECT_NAME}"
    req = urllib.request.Request(url, headers={"User-Agent": "yatws-release-preflight/0.1"})
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            published = {
                json.loads(line)["vers"]
                for line in resp.read().decode("utf-8").splitlines()
                if line.strip()
            }
        if version in published:
            return False, f"{PROJECT_NAME} {version} is already published on crates.io"
    except urllib.error.HTTPError as e:
        if e.code != 404:
            return False, f"crates.io index HTTP {e.code}"
    except Exception as e:
        return False, f"could not query crates.io sparse index: {e}"

    return True, f"{token_msg}; {PROJECT_NAME} {version} not yet on crates.io"


def test_git_tag_signing(git_gpg_format, git_signing_key):
    """Checks that the configured Git tag-signing key is ready without hanging on passphrase prompts."""
    expanded_key = os.path.expanduser(git_signing_key)
    if git_gpg_format == "ssh":
        if shutil.which("ssh-keygen") is None:
            return False, "`ssh-keygen` not found in PATH"
        if not os.path.exists(expanded_key):
            return False, f"SSH signing key file not found: {expanded_key}"

        pub_path = Path(expanded_key if expanded_key.endswith(".pub") else f"{expanded_key}.pub")
        if pub_path.exists() and shutil.which("ssh-add"):
            pub_parts = pub_path.read_text(encoding="utf-8").strip().split()
            pub_material = " ".join(pub_parts[:2])
            agent_keys = subprocess.run(
                ["ssh-add", "-L"],
                capture_output=True,
                text=True,
                check=False,
            )
            if agent_keys.returncode != 0 or pub_material not in agent_keys.stdout:
                priv_hint = str(pub_path).removesuffix(".pub")
                return (
                    False,
                    f"key not loaded in ssh-agent — run `ssh-add {priv_hint}`",
                )

            with tempfile.TemporaryDirectory() as tmpdir:
                payload = Path(tmpdir) / "preflight.txt"
                payload.write_text("yatws release preflight\n", encoding="utf-8")
                res = subprocess.run(
                    ["ssh-keygen", "-Y", "sign", "-f", str(pub_path), "-n", "git", str(payload)],
                    stdin=subprocess.DEVNULL,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                if res.returncode != 0:
                    err = res.stderr.strip() or res.stdout.strip()
                    return False, f"`ssh-keygen -Y sign` failed ({err})"

        return True, f"ssh ({git_signing_key})"

    res = subprocess.run(
        ["gpg", "--list-secret-keys", git_signing_key],
        capture_output=True,
        text=True,
        check=False,
    )
    if res.returncode != 0 or not res.stdout.strip():
        return False, f"GPG secret key '{git_signing_key}' not found"
    return True, f"{git_gpg_format} ({git_signing_key})"


def verify_prerequisites(
    version,
    sq_bin,
    pqc_key,
    expected_branch="main",
    allow_missing_git_key=False,
):
    """Runs all upfront sanity checks identically for both `--dry-run` and real releases."""
    print(f"\n=== Pre-flight Sanity Checks for {PROJECT_NAME} v{version} ===")
    failures = []

    def record(ok, label, detail=""):
        status = "[OK]  " if ok else "[FAIL]"
        suffix = f": {detail}" if detail else ""
        print(f"  {status} {label}{suffix}")
        if not ok:
            failures.append(f"{label}{suffix}")

    # 1. CLI tools
    missing_tools = [t for t in ("git", "cargo", "gh", sq_bin) if shutil.which(t) is None]
    audit_ok = (
        "cargo" not in missing_tools
        and subprocess.run(
            ["cargo", "audit", "--version"], capture_output=True, check=False
        ).returncode
        == 0
    )
    if missing_tools:
        record(False, "Required CLI tools", f"missing {', '.join(missing_tools)}")
    elif not audit_ok:
        record(
            False,
            "Required CLI tools",
            "`cargo audit` missing (install with `cargo install cargo-audit --locked`)",
        )
    else:
        record(True, "Required CLI tools", f"git, cargo, cargo-audit, gh, {sq_bin}")

    if missing_tools:
        print("\n[ABORT] Install missing CLI tools before continuing.")
        sys.exit(1)

    # 2. Version consistency
    mismatches = check_version_consistency(version)
    if mismatches:
        record(False, "Version consistency", f"expected {version}, found {', '.join(mismatches)}")
    else:
        record(
            True,
            "Version consistency",
            f"Cargo.toml, Cargo.lock, MODULE.bazel, llms.txt == {version}",
        )

    # 3. Clean git working tree & crate directory
    dirty_tracked = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=no"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    dirty_crate = subprocess.run(
        ["git", "status", "--porcelain", PROJECT_NAME],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    if dirty_tracked:
        files = ", ".join(line.split()[-1] for line in dirty_tracked.splitlines())
        record(False, "Clean git working tree", f"uncommitted changes in: {files}")
    elif dirty_crate:
        files = ", ".join(line.split()[-1] for line in dirty_crate.splitlines())
        record(False, "Clean crate directory", f"untracked files inside {PROJECT_NAME}/: {files}")
    else:
        record(True, "Clean git working tree", "no uncommitted changes")

    # 4. Branch & pushed-to-origin check (also verifies SSH access to origin!)
    branch_res = subprocess.run(
        ["git", "symbolic-ref", "--short", "-q", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    current_branch = branch_res.stdout.strip()
    head_sha = resolve_commit_sha("HEAD")
    if expected_branch and current_branch != expected_branch:
        record(
            False,
            "Git branch",
            f"on '{current_branch or 'detached HEAD'}', expected '{expected_branch}' "
            "(pass --allow-branch to override)",
        )
    else:
        record(True, "Git branch", f"{current_branch} ({head_sha[:10]})")

    tag_name = f"v{version}"
    local_tag_exists = (
        subprocess.run(
            ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag_name}"],
            capture_output=True,
            check=False,
        ).returncode
        == 0
    )

    ls_remote = subprocess.run(
        [
            "git",
            "ls-remote",
            "origin",
            f"refs/heads/{current_branch}" if current_branch else "HEAD",
            f"refs/tags/{tag_name}",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if ls_remote.returncode != 0:
        err_line = (ls_remote.stderr.strip().splitlines() or ["unknown error"])[0]
        record(
            False,
            "Origin SSH access",
            f"`git ls-remote origin` failed ({err_line}) — run `ssh-add ~/.ssh/github`",
        )
        # Fallback check via `gh api` so unpushed commits are still reported right away!
        gh_head = subprocess.run(
            ["gh", "api", f"repos/{{owner}}/{{repo}}/commits/{current_branch}", "--jq", ".sha"],
            capture_output=True,
            text=True,
            check=False,
        )
        remote_sha = gh_head.stdout.strip()
        if gh_head.returncode == 0 and remote_sha and remote_sha != head_sha:
            record(
                False,
                "Origin branch pushed",
                f"local HEAD ({head_sha[:10]}) != origin/{current_branch} ({remote_sha[:10]}) "
                f"— run `git push origin {current_branch}` first",
            )
    else:
        remote_refs = {}
        for line in ls_remote.stdout.splitlines():
            parts = line.split()
            if len(parts) == 2:
                remote_refs[parts[1]] = parts[0]

        remote_branch_sha = remote_refs.get(f"refs/heads/{current_branch}")
        if not remote_branch_sha:
            record(
                False,
                "Origin branch pushed",
                f"origin/{current_branch} does not exist yet — run `git push -u origin {current_branch}`",
            )
        elif remote_branch_sha != head_sha:
            record(
                False,
                "Origin branch pushed",
                f"local HEAD ({head_sha[:10]}) != origin/{current_branch} ({remote_branch_sha[:10]}) "
                f"— run `git push origin {current_branch}` first",
            )
        else:
            record(True, "Origin branch pushed", f"HEAD == origin/{current_branch} ({head_sha[:10]})")

        remote_tag_exists = f"refs/tags/{tag_name}" in remote_refs
        if local_tag_exists or remote_tag_exists:
            where = "locally and on origin" if (local_tag_exists and remote_tag_exists) else (
                "locally" if local_tag_exists else "on origin"
            )
            record(False, f"Tag {tag_name} unused", f"already exists {where}")
        else:
            record(True, f"Tag {tag_name} unused", "free locally and on origin")

    # 5. GitHub CLI auth & release uniqueness
    gh_auth = subprocess.run(["gh", "auth", "status"], capture_output=True, text=True, check=False)
    if gh_auth.returncode != 0:
        record(False, "GitHub CLI auth", "`gh auth status` failed — run `gh auth login`")
    else:
        gh_rel = subprocess.run(
            ["gh", "release", "view", tag_name],
            capture_output=True,
            text=True,
            check=False,
        )
        if gh_rel.returncode == 0:
            record(False, "GitHub Release unused", f"GitHub Release {tag_name} already exists")
        else:
            record(True, "GitHub CLI & Release", f"authenticated; Release {tag_name} not yet created")

    # 6. Crates.io credentials & version uniqueness
    crates_ok, crates_msg = check_crates_io(version)
    record(crates_ok, "Crates.io readiness", crates_msg)

    # 7. Git tag signing key (live signature check)
    git_signing_key = git_config_get("user.signingkey")
    git_gpg_format = git_config_get("gpg.format") or "openpgp"
    if allow_missing_git_key:
        record(True, "Git tag signing", "annotated only (--unsigned-git-tag)")
    elif not git_signing_key:
        record(
            False,
            "Git tag signing",
            "`user.signingkey` unset (configure SSH signing key or pass --unsigned-git-tag)",
        )
    else:
        sig_ok, sig_msg = test_git_tag_signing(git_gpg_format, git_signing_key)
        record(sig_ok, "Git tag signing", sig_msg)

    # 8. Sequoia PQC key
    pqc_fpr, pqc_algo, pqc_err = resolve_pqc_key(sq_bin, pqc_key)
    if pqc_err:
        record(False, "PQC artifact key", pqc_err)
    else:
        unlock_ok, unlock_msg = unlock_pqc_key(sq_bin, pqc_fpr)
        record(unlock_ok, "PQC artifact key", f"{pqc_fpr[:16]}... ({pqc_algo}) {unlock_msg}")

    # 9. Unit tests (`cargo test --lib --all-features`)
    test_res = subprocess.run(
        ["cargo", "test", "-p", PROJECT_NAME, "--lib", "--all-features", "--quiet"],
        capture_output=True,
        text=True,
        check=False,
    )
    if test_res.returncode != 0:
        record(False, "Unit tests", "`cargo test -p yatws --lib --all-features` failed")
    else:
        record(True, "Unit tests", "`cargo test -p yatws --lib --all-features` passed")

    # 10. Cargo audit check (verifies 0 vulnerabilities upfront even in --dry-run)
    if audit_ok:
        audit_res = subprocess.run(
            ["cargo", "audit"],
            capture_output=True,
            text=True,
            check=False,
        )
        if audit_res.returncode != 0:
            record(False, "Cargo security audit", "`cargo audit` reported vulnerabilities")
        else:
            record(True, "Cargo security audit", "`cargo audit` passed (0 vulnerabilities)")

    if failures:
        print(f"\n[ABORT] {len(failures)} pre-flight sanity check(s) failed.")
        sys.exit(1)

    print("All pre-flight sanity checks passed!\n")
    return pqc_fpr, pqc_algo, git_gpg_format, git_signing_key, audit_res.stdout


def generate_sha256sums(directory, tag_name, commit_sha):
    """Generates a SHA256SUMS file (including git tag & commit metadata) for all release artifacts."""
    print("[*] Generating SHA256SUMS...")
    sums_file = directory / "SHA256SUMS"
    with open(sums_file, "w") as f_out:
        # `sha256sum -c` ignores `#` comment lines, while cleartext-signing binds the
        # exact Git tag and commit hash under the quantum-safe ML-DSA signature.
        f_out.write(f"# project: {PROJECT_NAME}\n")
        f_out.write(f"# git-tag: {tag_name}\n")
        f_out.write(f"# git-commit: {commit_sha}\n")
        for filepath in sorted(directory.glob("*")):
            if filepath.name == "SHA256SUMS" or filepath.is_dir():
                continue

            sha256_hash = hashlib.sha256()
            with open(filepath, "rb") as f_in:
                for byte_block in iter(lambda: f_in.read(4096), b""):
                    sha256_hash.update(byte_block)

            # Standard sha256sum format: <hash>  <filename>
            f_out.write(f"{sha256_hash.hexdigest()}  {filepath.name}\n")
    return sums_file


def parse_args():
    parser = argparse.ArgumentParser(
        description=(
            "Build, sign (GitHub-verified Git tag + sq-pqc quantum-safe artifact signatures), "
            "publish to crates.io, and create a GitHub release for yatws."
        )
    )
    parser.add_argument(
        "version",
        nargs="?",
        help=(
            "Release version without leading 'v' (e.g. 0.1.10), or 'bump' to bump version files "
            "in the working tree and exit."
        ),
    )
    parser.add_argument(
        "bump_target",
        nargs="?",
        help="Optional target version or 'patch'/'minor'/'major' when using 'bump' (default: bump last number).",
    )
    parser.add_argument(
        "--bump",
        action="store_true",
        help=(
            "Alias for 'bump': bump version in yatws/Cargo.toml, MODULE.bazel, Cargo.lock, and llms.txt "
            "in the working tree (defaults to incrementing the last version number) and exit"
        ),
    )
    parser.add_argument(
        "--sq-bin",
        default="sq-pqc",
        help="Sequoia PQC binary name or path (default: sq-pqc)",
    )
    parser.add_argument(
        "--pqc-key",
        help="OpenPGP v6 PQC certificate fingerprint for artifact signing (auto-detected from git user.email if omitted)",
    )
    parser.add_argument(
        "--allow-branch",
        help="Allow releasing from a branch other than 'main'",
    )
    parser.add_argument(
        "--unsigned-git-tag",
        action="store_true",
        help="Create an annotated git tag (`git tag -a`) without SSH/GPG tag signing (artifacts remain PQC-signed)",
    )
    note_group = parser.add_mutually_exclusive_group()
    note_group.add_argument(
        "--note",
        help="Text (Markdown) to place at the beginning of the GitHub release notes",
    )
    note_group.add_argument(
        "--note-file",
        type=Path,
        help="File whose contents (Markdown) are placed at the beginning of the GitHub release notes",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Run all pre-flight checks (including push sync, SSH sign, tests, audit, package) without tagging or publishing",
    )
    return parser.parse_args()


def main():
    ensure_workspace_root()
    ensure_local_bins_in_path()
    configure_gpg_tty()
    args = parse_args()

    if args.bump or args.version == "bump":
        target = args.bump_target if args.version == "bump" else args.version
        bump_version(target=target, dry_run=args.dry_run)
        return

    if not args.version:
        print("[ERROR] Release version is required (or run `release.py bump` to bump the version).")
        sys.exit(1)

    version = args.version.lstrip("v")
    tag_name = f"v{version}"
    expected_branch = args.allow_branch if args.allow_branch else "main"

    release_note = args.note
    if args.note_file:
        try:
            release_note = args.note_file.read_text(encoding="utf-8")
        except OSError as e:
            print(f"[ERROR] Could not read --note-file: {e}")
            sys.exit(1)
    release_note = (release_note or "").strip()

    pqc_fpr, pqc_algo, git_gpg_format, git_signing_key, audit_report = verify_prerequisites(
        version,
        sq_bin=args.sq_bin,
        pqc_key=args.pqc_key,
        expected_branch=expected_branch,
        allow_missing_git_key=args.unsigned_git_tag,
    )

    # Package & verify the crate upfront (even in `--dry-run`!) before creating any git tags.
    print("--- Step 1: Cargo Package Verification ---")
    print("[*] Running: cargo package -p yatws")
    subprocess.run(["cargo", "package", "-p", PROJECT_NAME], check=True)
    crate_source = Path(f"target/package/{PROJECT_NAME}-{version}.crate")
    if not crate_source.exists():
        print(f"[ERROR] Expected packaged crate not found at {crate_source}")
        sys.exit(1)

    if not args.dry_run:
        if ASSETS_DIR.exists():
            shutil.rmtree(ASSETS_DIR)
        ASSETS_DIR.mkdir()

    print(f"\n=== Release Pipeline for {PROJECT_NAME} {tag_name} ===")
    print(f"Workspace:        {Path.cwd()}")
    print(f"PQC artifact key: {pqc_fpr} ({pqc_algo})")
    if args.unsigned_git_tag:
        print("Git tag signing:  annotated only (--unsigned-git-tag)")
    else:
        print(f"Git tag signing:  {git_gpg_format} ({git_signing_key})")
    print()

    # 2. Save Cargo Audit Report
    print("--- Step 2: Cargo Audit Report ---")
    audit_file = ASSETS_DIR / "cargo_audit_report.txt"
    if args.dry_run:
        print(f"[dry-run] Would write verified cargo audit report -> {audit_file}")
    else:
        audit_file.write_text(audit_report, encoding="utf-8")
        print(f"[*] Wrote cargo audit report to {audit_file}")

    # 3. Git Tag & Export PQC Public Key + Source Archive
    print("\n--- Step 3: Git Tag & Quantum-Safe Sign Source ---")
    git_tag_flag = "-a" if args.unsigned_git_tag else "-s"
    run_cmd(
        ["git", "tag", git_tag_flag, tag_name, "-m", f"Release {tag_name}"],
        dry_run=args.dry_run,
    )

    pqc_pubkey_path = ASSETS_DIR / PQC_PUBKEY_FILENAME
    run_cmd(
        [args.sq_bin, "cert", "export", "--cert", pqc_fpr, "--output", str(pqc_pubkey_path)],
        dry_run=args.dry_run,
        cleanup_local_tag=tag_name,
    )

    source_tar = ASSETS_DIR / f"{PROJECT_NAME}-{version}-source.tar.gz"
    run_cmd(
        [
            "git",
            "archive",
            "--format=tar.gz",
            f"--prefix={PROJECT_NAME}-{version}/",
            tag_name,
            "-o",
            str(source_tar),
        ],
        dry_run=args.dry_run,
        cleanup_local_tag=tag_name,
    )
    run_cmd(
        [
            args.sq_bin,
            "sign",
            "--signer",
            pqc_fpr,
            "--signature-file",
            f"{source_tar}.asc",
            str(source_tar),
        ],
        dry_run=args.dry_run,
        cleanup_local_tag=tag_name,
    )

    # 4. Copy & Sign Packaged Crate + Publish
    print("\n--- Step 4: Quantum-Safe Sign Crate & Publish ---")
    crate_dest = ASSETS_DIR / crate_source.name
    if args.dry_run:
        print(f"[dry-run] Would copy {crate_source} -> {crate_dest}")
    else:
        shutil.copy(crate_source, crate_dest)

    run_cmd(
        [
            args.sq_bin,
            "sign",
            "--signer",
            pqc_fpr,
            "--signature-file",
            f"{crate_dest}.asc",
            str(crate_dest),
        ],
        dry_run=args.dry_run,
        cleanup_local_tag=tag_name,
    )

    run_cmd(
        ["cargo", "publish", "-p", PROJECT_NAME],
        dry_run=args.dry_run,
        cleanup_local_tag=tag_name,
    )

    # 5. Generate and Quantum-Safe Cleartext-Sign Checksums
    print("\n--- Step 5: Checksums (PQC Cleartext-Signed) ---")
    sums_file = ASSETS_DIR / "SHA256SUMS"
    sums_asc = ASSETS_DIR / "SHA256SUMS.asc"
    if args.dry_run:
        commit_sha = resolve_commit_sha("HEAD")
        print(
            f"[dry-run] Would generate {sums_file} (binding commit {commit_sha}) "
            f"and PQC cleartext-sign -> {sums_asc}"
        )
    else:
        commit_sha = resolve_commit_sha(tag_name)
        sums_file = generate_sha256sums(ASSETS_DIR, tag_name=tag_name, commit_sha=commit_sha)
        run_cmd(
            [
                args.sq_bin,
                "sign",
                "--signer",
                pqc_fpr,
                "--cleartext",
                "--output",
                str(sums_asc),
                str(sums_file),
            ],
            cleanup_local_tag=tag_name,
        )
        sums_file.unlink()

    # 6. GitHub Release
    print("\n--- Step 6: GitHub Release ---")
    run_cmd(["git", "push", "origin", tag_name], dry_run=args.dry_run)

    release_files = (
        [f"{ASSETS_DIR}/<artifacts>"]
        if args.dry_run
        else sorted(str(p) for p in ASSETS_DIR.glob("*"))
    )

    release_notes = (
        f"Quantum-safe (`{pqc_algo}`, OpenPGP v6 / RFC 9980) release artifacts included.\n\n"
        f"- **PQC Signing Key Fingerprint:** `{pqc_fpr}`\n"
        f"- **Verify all artifacts & commit binding with Sequoia PQC (`sq`):**\n"
        f"  ```sh\n"
        f"  sq verify --signer-file {PQC_PUBKEY_FILENAME} --cleartext SHA256SUMS.asc | sha256sum -c -\n"
        f"  ```"
    )
    if release_note:
        release_notes = f"{release_note}\n\n---\n\n{release_notes}"

    if args.dry_run:
        print("[dry-run] Release notes:")
        for line in release_notes.splitlines():
            print(f"    {line}")

    gh_cmd = [
        "gh",
        "release",
        "create",
        tag_name,
        "--title",
        f"Release {tag_name}",
        "--notes",
        release_notes,
    ] + release_files

    run_cmd(gh_cmd, dry_run=args.dry_run)

    print("\n=== Dry-Run Complete! ===" if args.dry_run else "\n=== Release Complete! ===")
    if not args.dry_run:
        print("Artifacts pushed to GitHub and crates.io.")
        print("Keep your manual backups of the .asc files safe.")


if __name__ == "__main__":
    main()
