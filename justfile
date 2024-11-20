forge := "forge"
cargo := "cargo"

@full-run: compile run-tests

@compile:
    {{forge}} compile -C solidity

@run-tests:
    {{cargo}} nextest run --no-capture # -j1 is enabled by no-capture
