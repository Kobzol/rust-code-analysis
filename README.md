# Rust code analysis

Various scripts for analysing Rust code in the wild

## scan-from-impls
This program scans the top 100 crates.io crates and counts how many single-field tuple structs implement `From<FieldType>`.

## format-args
This program scans the top N crates.io crates and counts which kinds of expressions are used as arguments to formatting macros.

## Other
Most starred Rust repos: https://api.github.com/search/repositories?q=language:rust&sort=stars&order=desc
