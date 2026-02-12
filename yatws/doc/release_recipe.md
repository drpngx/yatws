# How to release

## Check the build
```bash
bazel build -c opt //yatws/...
```
Then, run `bazel-bin/yatws/gen_goldens` to verify that it works against
the paper IBKR TWS. Check the positions and orders to make sure there is
no garbage left. It will output the diagnostics markdown that needs to
be checked in `yatws/doc/test_results.md`.

## Update the documentation
```bash
bazel run //yatws:copy_llms_txt
```

Ask gemini to update `yatws/doc/api.md`
Ask gemini to update `yatws/src/lib.rs`.
Ask gemini to update `yatws/README.md`

## Bump the revision
In `MODULE.bazel` and `yatws/Cargo.toml`

## Git
Push to git, create a new release with the tag.

## Push the crate
```
cargo publish -p yatws
```
This actually publishes the crate.
