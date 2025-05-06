load("@rules_python//python:pip.bzl", "compile_pip_requirements")
load("@yatws_pip_deps//:requirements.bzl", "requirement")

# Run this to update requirements_lock.txt with pinned versions and hashes.
compile_pip_requirements(
    name = "requirements",
    requirements_in = "requirements.txt",
    requirements_txt = "requirements_lock.txt",
)

# For testing purposes only.
# You have to download the toolkit from IBKR.
py_library(
    name = "ibapi",
    srcs = glob(["IBJts/source/pythonclient/ibapi/**/*.py"], allow_empty=True),
    imports = ["IBJts/source/pythonclient"],
    visibility = ["//visibility:public"],
)
