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
    ],
    edition = "2021",
    proc_macro_deps = [
        "@crates//:async-trait",
    ],
    deps = [
        "@crates//:chrono",
        "@crates//:thiserror",
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
