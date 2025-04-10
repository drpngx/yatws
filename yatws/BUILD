load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test", "rust_doc", "rust_binary")

package(default_visibility = ["//visibility:public"])

rust_library(
    name = "yatws",
    srcs = [
        "src/lib.rs",
        "src/base.rs",
        "src/contract.rs",
        "src/order.rs",
        "src/account.rs",
        "src/protocol.rs",
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
    ],
    edition = "2021",
    proc_macro_deps = [
        "@crates//:async-trait",
    ],
    deps = [
        "@crates//:chrono",
        "@crates//:thiserror",
        "@crates//:num-traits",
        "@crates//:byteorder",
        "@crates//:serde",
        "@crates//:bincode",
        "@crates//:tokio",
        "@crates//:futures",
        "@crates//:parking_lot",
        "@crates//:log",
        "@crates//:env_logger",
        "@crates//:rusqlite",
        "@crates//:lazy_static",
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
        "@crates//:env_logger",
    ],
    visibility = ["//visibility:public"],
)
