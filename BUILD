load("@rules_python//python:pip.bzl", "compile_pip_requirements")
load("@yatws_pip_deps//:requirements.bzl", "requirement")

# Run this to update requirements_lock.txt with pinned versions and hashes.
compile_pip_requirements(
    name = "requirements",
    requirements_in = "requirements.txt",
    requirements_txt = "requirements_lock.txt",
)

py_binary(
    name = "gemini",
    srcs = ["gemini.py"],
    deps = [
        requirement("google-genai"),
    ],
)
