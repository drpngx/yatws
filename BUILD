load("@rules_python//python:defs.bzl", "py_binary", "py_library")
load("@rules_python//python:pip.bzl", "compile_pip_requirements")
load("@yatws_pip_deps//:requirements.bzl", "requirement")

# Run this to update requirements_lock.txt with pinned versions and hashes.
compile_pip_requirements(
    name = "requirements",
    requirements_in = "requirements.txt",
    requirements_txt = "requirements_lock.txt",
)

# Release pipeline: `bazel run //:release -- <version>` (or `--dry-run <version>`).
py_binary(
    name = "release",
    srcs = ["release.py"],
    main = "release.py",
    visibility = ["//visibility:public"],
)

# For testing purposes only.
# You have to download the toolkit from IBKR.
py_library(
    name = "ibapi",
    srcs = glob(["IBJts/source/pythonclient/ibapi/**/*.py"], allow_empty=True),
    imports = ["IBJts/source/pythonclient"],
    visibility = ["//visibility:public"],
)
