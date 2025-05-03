# Basic tests to check against the reference implementation.
# Make sure you download the IBAPI and unpack it.
package(default_visibility = ["//visibility:public"])

py_binary(
    name = "parse_message",
    srcs = ["parse_message.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "single_quote",
    srcs = ["single_quote.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "box_spread_yield",
    srcs = ["box_spread_yield.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "historical_data",
    srcs = ["historical_data.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "realtime_data",
    srcs = ["realtime_data.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "broker_cleanup_orders",
    srcs = ["broker_cleanup_orders.py"],
    deps = ["//:ibapi"],
)

py_binary(
    name = "broker_order_market",
    srcs = ["broker_order_market.py"],
    deps = ["//:ibapi"],
)
