forge := "forge"
cargo := "cargo"

@full-run: compile run-tests

@compile:
    {{forge}} compile -C solidity

@run-tests:
    {{cargo}} nextest run --test-threads 1
