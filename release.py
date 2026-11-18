#!/usr/bin/env python3

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

PROJECT_NAME = "yatws"
ASSETS_DIR = Path("release_artifacts")
PQC_PUBKEY_FILENAME = f"{PROJECT_NAME}-pqc-pubkey.asc"


def run_cmd(cmd, shell=False, cwd=None, dry_run=False):
    """Executes a CLI command and halts on failure."""
    rendered = " ".join(cmd) if isinstance(cmd, list) else cmd
    if dry_run:
        print(f"[dry-run] Would run: {rendered}")
        return
    print(f"[*] Running: {rendered}")
    try:
        subprocess.run(cmd, shell=shell, check=True, cwd=cwd)
    except subprocess.CalledProcessError as e:
        print(f"\n[ERROR] Command failed: {e}")
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


def resolve_pqc_key(sq_bin, cli_pqc_key):
    """Resolves the OpenPGP v6 PQC key fingerprint and algorithm from `sq-pqc`."""
    export_cmd = [sq_bin, "cert", "export"]
    if cli_pqc_key:
        export_cmd += ["--cert", cli_pqc_key]
    else:
        email = git_config_get("user.email")
        if not email:
            print("[ERROR] `git config user.email` is unset and `--pqc-key` was not provided.")
            sys.exit(1)
        export_cmd += ["--cert-email", email]

    exported = subprocess.run(export_cmd, capture_output=True, check=False)
    if exported.returncode != 0 or not exported.stdout:
        target = cli_pqc_key or git_config_get("user.email")
        print(f"[ERROR] Could not find certificate for '{target}' in `{sq_bin}` cert store.")
        sys.exit(1)

    inspected = subprocess.run(
        [sq_bin, "inspect"],
        input=exported.stdout,
        capture_output=True,
        check=False,
    )
    text = inspected.stdout.decode("utf-8", errors="replace")

    # Parse primary certificate blocks: pick a v6 (64-hex) / PQC cert if multiple exist.
    candidates = []
    for block in text.split("OpenPGP Certificate.")[1:]:
        fpr_match = re.search(r"Fingerprint:\s*([0-9A-Fa-f]{40,64})", block)
        algo_match = re.search(r"Public-key algo:\s*(.+)", block)
        if fpr_match:
            fpr = fpr_match.group(1).upper()
            algo = algo_match.group(1).strip() if algo_match else "unknown"
            candidates.append((fpr, algo))

    if not candidates:
        print(f"[ERROR] Unable to parse certificate fingerprint from `{sq_bin} inspect`.")
        sys.exit(1)

    pqc_candidates = [
        (fpr, algo)
        for fpr, algo in candidates
        if len(fpr) == 64 or "ML-DSA" in algo or "SLH-DSA" in algo
    ]
    selected_fpr, selected_algo = pqc_candidates[0] if pqc_candidates else candidates[0]

    # Verify the private key material is available in `sq-pqc key list`.
    key_list = subprocess.run(
        [sq_bin, "key", "list"],
        capture_output=True,
        text=True,
        check=False,
    )
    if key_list.returncode != 0 or selected_fpr not in key_list.stdout.upper():
        print(f"[ERROR] Secret key for {selected_fpr} not found in `{sq_bin} key list`.")
        sys.exit(1)

    return selected_fpr, selected_algo


def verify_prerequisites(version, sq_bin, pqc_key, allow_missing_git_key=False, dry_run=False):
    """Checks tools, keys, git state, and crate version before mutating tags or publishing."""
    for tool in ("git", "cargo", "gh", sq_bin):
        if shutil.which(tool) is None:
            print(f"[ERROR] Required tool '{tool}' not found in PATH.")
            sys.exit(1)

    audit_check = subprocess.run(
        ["cargo", "audit", "--version"],
        capture_output=True,
        text=True,
        check=False,
    )
    if audit_check.returncode != 0:
        print(
            "[ERROR] `cargo audit` is not installed.\n"
            "        Install it first with: cargo install cargo-audit --locked"
        )
        sys.exit(1)

    pqc_fpr, pqc_algo = resolve_pqc_key(sq_bin, pqc_key)

    git_signing_key = git_config_get("user.signingkey")
    git_gpg_format = git_config_get("gpg.format") or "openpgp"
    if not git_signing_key and not allow_missing_git_key:
        print(
            "[ERROR] Git tag signing key (`user.signingkey`) is not configured.\n"
            "        Note: GitHub does not verify OpenPGP v6 / PQC keys on Git tags yet.\n"
            "        Recommended setup using your existing SSH key for GitHub's 'Verified' badge:\n"
            "          gh auth refresh -h github.com -s admin:ssh_signing_key\n"
            "          gh ssh-key add ~/.ssh/github.pub --type signing --title 'yatws release tag signing'\n"
            "          git config --global gpg.format ssh\n"
            "          git config --global user.signingkey ~/.ssh/github.pub"
        )
        sys.exit(1)

    cargo_toml = Path(PROJECT_NAME) / "Cargo.toml"
    with open(cargo_toml, "rb") as f:
        manifest = tomllib.load(f)
    crate_version = manifest.get("package", {}).get("version")
    if crate_version != version:
        print(
            f"[ERROR] Version mismatch: {cargo_toml} has version '{crate_version}', "
            f"but release requested '{version}'."
        )
        sys.exit(1)

    tag_name = f"v{version}"
    tag_check = subprocess.run(
        ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag_name}"],
        capture_output=True,
        text=True,
        check=False,
    )
    if tag_check.returncode == 0:
        print(f"[ERROR] Git tag '{tag_name}' already exists locally.")
        sys.exit(1)

    dirty_check = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=no"],
        capture_output=True,
        text=True,
        check=False,
    )
    if dirty_check.stdout.strip() and not dry_run:
        print(
            "[ERROR] Working tree has uncommitted changes (cargo package will reject dirty state).\n"
            "        Commit your changes first before running release."
        )
        sys.exit(1)

    return pqc_fpr, pqc_algo, git_gpg_format, git_signing_key


def resolve_commit_sha(ref):
    res = subprocess.run(
        ["git", "rev-parse", f"{ref}^{{commit}}"],
        capture_output=True,
        text=True,
        check=True,
    )
    return res.stdout.strip()


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
        help="Release version without leading 'v' (e.g. 0.1.8)",
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
        "--unsigned-git-tag",
        action="store_true",
        help="Create an annotated git tag (`git tag -a`) without SSH/GPG tag signing (artifacts remain PQC-signed)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate prerequisites and print release commands without tagging or publishing",
    )
    return parser.parse_args()


def main():
    ensure_workspace_root()
    ensure_local_bins_in_path()
    configure_gpg_tty()
    args = parse_args()

    version = args.version.lstrip("v")
    tag_name = f"v{version}"

    pqc_fpr, pqc_algo, git_gpg_format, git_signing_key = verify_prerequisites(
        version,
        sq_bin=args.sq_bin,
        pqc_key=args.pqc_key,
        allow_missing_git_key=args.unsigned_git_tag,
        dry_run=args.dry_run,
    )

    # 0. Prep Environment
    if not args.dry_run:
        if ASSETS_DIR.exists():
            shutil.rmtree(ASSETS_DIR)
        ASSETS_DIR.mkdir()

    print(f"\n=== Starting Release Pipeline for {PROJECT_NAME} {tag_name} ===")
    print(f"Workspace:        {Path.cwd()}")
    print(f"PQC artifact key: {pqc_fpr} ({pqc_algo})")
    if args.unsigned_git_tag:
        print("Git tag signing:  annotated only (--unsigned-git-tag)")
    else:
        print(f"Git tag signing:  {git_gpg_format} ({git_signing_key})")
    print()

    # 1. Cargo Audit
    print("--- Step 1: Cargo Audit ---")
    audit_file = ASSETS_DIR / "cargo_audit_report.txt"
    if args.dry_run:
        print(f"[dry-run] Would run: cargo audit > {audit_file}")
    else:
        with open(audit_file, "w") as f:
            print(f"[*] Running cargo audit and writing to {audit_file.name}")
            try:
                subprocess.run(
                    ["cargo", "audit"],
                    stdout=f,
                    stderr=subprocess.STDOUT,
                    check=True,
                )
            except subprocess.CalledProcessError as e:
                print(f"\n[ERROR] `cargo audit` failed (see {audit_file}): {e}")
                sys.exit(1)

    # 2. Git Tag & Export PQC Public Key + Source Archive
    print("\n--- Step 2: Git Tag & Quantum-Safe Sign Source ---")
    git_tag_flag = "-a" if args.unsigned_git_tag else "-s"
    run_cmd(
        ["git", "tag", git_tag_flag, tag_name, "-m", f"Release {tag_name}"],
        dry_run=args.dry_run,
    )

    pqc_pubkey_path = ASSETS_DIR / PQC_PUBKEY_FILENAME
    run_cmd(
        [args.sq_bin, "cert", "export", "--cert", pqc_fpr, "--output", str(pqc_pubkey_path)],
        dry_run=args.dry_run,
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
    )

    # 3. Cargo Package, Sign, and Publish
    print("\n--- Step 3: Cargo Package & Quantum-Safe Sign ---")
    run_cmd(["cargo", "package", "-p", PROJECT_NAME], dry_run=args.dry_run)

    crate_source = Path(f"target/package/{PROJECT_NAME}-{version}.crate")
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
    )

    print("\n--- Step 3b: Cargo Publish ---")
    run_cmd(["cargo", "publish", "-p", PROJECT_NAME], dry_run=args.dry_run)

    # 4. Generate and Quantum-Safe Cleartext-Sign Checksums
    print("\n--- Step 4: Checksums (PQC Cleartext-Signed) ---")
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
            ]
        )
        # Clean up the unsigned SUMS file so we only upload the signed one (.asc)
        sums_file.unlink()

    # 5. GitHub Release
    print("\n--- Step 5: GitHub Release ---")
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

    print("\n=== Release Complete! ===")
    if not args.dry_run:
        print("Artifacts pushed to GitHub and crates.io.")
        print("Keep your manual backups of the .asc files safe.")


if __name__ == "__main__":
    main()
