load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test", "rust_doc", "rust_binary")

package(default_visibility = ["//visibility:public"])

rust_library(
    name = "yatws",
    srcs = [
        "src/lib.rs",
        "src/base.rs",
        "src/contract.rs",
        "src/order.rs",
        "src/order_builder.rs",
        "src/account.rs",
        "src/news.rs",
        "src/data.rs",
        "src/data_wsh.rs",
        "src/financial_report_parser.rs",
        "src/protocol_encoder.rs",
        "src/protocol_decoder.rs",
        "src/protocol_dec_parser.rs",
        "src/handler.rs",
        "src/message_parser.rs",
        "src/parser_client.rs",
        "src/parser_account.rs",
        "src/parser_order.rs",
        "src/parser_fin_adv.rs",
        "src/parser_data_ref.rs",
        "src/parser_data_market.rs",
        "src/parser_data_fin.rs",
        "src/parser_data_news.rs",
        "src/min_server_ver.rs",
        "src/conn.rs",
        "src/conn_log.rs",
        "src/conn_mock.rs",
        "src/client.rs",
        "src/order_manager.rs",
        "src/account_manager.rs",
        "src/data_ref_manager.rs",
        "src/data_market_manager.rs",
        "src/data_news_manager.rs",
        "src/data_fin_manager.rs",
        "src/options_strategy_builder.rs", # Add new file
    ],
    edition = "2021",
    proc_macro_deps = [
        "@crates//:async-trait",
        "@crates//:bitmask-enum",
    ],
    deps = [
        "@crates//:bincode",
        "@crates//:byteorder",
        "@crates//:chrono",
        "@crates//:chrono-tz",
        "@crates//:env_logger",
        "@crates//:futures",
        "@crates//:lazy_static",
        "@crates//:log",
        "@crates//:num_enum",
        "@crates//:num-traits",
        "@crates//:once_cell",
        "@crates//:parking_lot",
        "@crates//:quick-xml",
        "@crates//:rusqlite",
        "@crates//:rust_decimal",
        "@crates//:serde",
        "@crates//:serde_json",
        "@crates//:socket2",
        "@crates//:thiserror",
        "@crates//:tokio",
        "@crates//:uuid",
    ],
)

rust_test(
    name = "yatws_test",
    crate = ":yatws",
    deps = [
        "@crates//:tokio-test",
        "@crates//:mockall",
        "@crates//:tempfile",
        "@crates//:serial_test",
    ],
)

rust_doc(
    name = "yatws_docs",
    crate = ":yatws",
)

rust_binary(
    name = "tws_snoop",
    srcs = ["tws_snoop.rs"],
    edition = "2021",
    deps = [
        ":yatws",
        "@crates//:byteorder",
        "@crates//:clap",
        "@crates//:log",
        "@crates//:env_logger",
    ],
    visibility = ["//visibility:public"],
)

rust_binary(
    name = "gen_goldens",
    srcs = ["gen_goldens.rs"],
    edition = "2021",
    deps = [
        ":yatws",
        "@crates//:anyhow",
        "@crates//:chrono",
        "@crates//:clap",
        "@crates//:env_logger",
        "@crates//:inventory",
        "@crates//:log",
        "@crates//:once_cell",
    ],
    visibility = ["//visibility:public"],
)
