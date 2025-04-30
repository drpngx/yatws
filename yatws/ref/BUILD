# Basic tests to check against the reference implementation.
# Make sure you download the IBAPI and unpack it.
package(default_visibility = ["//visibility:public"])

py_binary(
    name = "single_quote",
    srcs = ["single_quote.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "realtime_data",
    srcs = ["realtime_data.py"],
    deps = ["//:ibapi"],
)
