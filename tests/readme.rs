//! The README's example is the crate-level doctest. Only the doctest is compiled, so this
//! keeps the copy in the README from drifting away from the code that is actually checked.

/// The fenced block between `open` and `close`. In a doc comment every line carries `//!`
/// followed by one space, except a blank line, which is `//!` alone; both come off here.
fn snippet(text: &str, open: &str, close: &str, doc_comment: bool) -> String {
    let start = text.find(open).expect("example block") + open.len();
    let body = &text[start..];
    let end = body.find(close).expect("example block end");
    body[..end]
        .lines()
        .map(|line| {
            let line = match doc_comment {
                true => {
                    let l = line.strip_prefix("//!").expect("doc comment line");
                    l.strip_prefix(' ').unwrap_or(l)
                }
                false => line,
            };
            line.trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn readme_example_matches_the_doctest() {
    let readme = snippet(include_str!("../README.md"), "```rust\n", "```", false);
    let doctest = snippet(
        include_str!("../src/lib.rs"),
        "//! ```no_run\n",
        "//! ```",
        true,
    );
    assert_eq!(
        readme.trim(),
        doctest.trim(),
        "README example and the src/lib.rs doctest have drifted"
    );
}
