use std::path::Path;
use uncomment::Processor;
use uncomment::config::ResolvedConfig;
use uncomment::tree_sitter::{InputEdit, Language, Parser, Point, Tree};

fn resolved_config() -> ResolvedConfig {
    ResolvedConfig {
        remove_todos: false,
        remove_fixme: false,
        remove_docs: false,
        preserve_patterns: Vec::new(),
        use_default_ignores: true,
        respect_gitignore: true,
        traverse_git_repos: false,
        language_config: None,
    }
}

const SAMPLES: &[(&str, &str)] = &[
    (
        "sample.rs",
        "// remove me\nfn main() {\n    let x = 1; // trailing\n    // TODO: keep\n    /* block */\n}\n",
    ),
    (
        "sample.py",
        "# remove me\ndef f():\n    \"\"\"docstring\"\"\"\n    x = 1  # trailing\n    # noqa: E501\n    return x\n",
    ),
    (
        "sample.go",
        "package main\n\n// remove me\nfunc main() {\n\tx := 1 // trailing\n\t/* block */\n\t_ = x\n}\n",
    ),
    (
        "sample.ts",
        "// remove me\nconst x: number = 1; // trailing\n/* block */\nexport default x;\n",
    ),
];

/// Parses with a parser the caller owns, as a host holding its own shared tree would.
fn external_parse(source: &str, language: &Language) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    parser.parse(source, None)
}

#[test]
fn plan_removals_with_external_tree_matches_plan_removals() {
    for (name, source) in SAMPLES {
        let path = Path::new(name);
        let expected = Processor::new()
            .plan_removals(source, path, &resolved_config())
            .expect("plan removals");
        assert!(!expected.is_empty(), "{name}: the sample has removable comments");

        let mut called = false;
        let actual = Processor::new()
            .plan_removals_with(source, path, &resolved_config(), |language, _| {
                called = true;
                external_parse(source, language)
            })
            .expect("plan removals with an external tree");
        assert!(called, "{name}: the parse closure ran");
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn inspect_with_external_tree_matches_inspect() {
    for (name, source) in SAMPLES {
        let path = Path::new(name);
        let expected = Processor::new()
            .inspect(source, path, &resolved_config())
            .expect("inspect");
        let actual = Processor::new()
            .inspect_with(source, path, &resolved_config(), |language, _| {
                external_parse(source, language)
            })
            .expect("inspect with an external tree");
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{name}");
    }
}

#[test]
fn plan_removals_with_reparses_incrementally_on_the_processor_parser() {
    let old_source = "fn main() {\n    let x = 1; // trailing\n}\n";
    let new_source = "// added\nfn main() {\n    let x = 1; // trailing\n}\n";
    let path = Path::new("sample.rs");

    let rust = tree_sitter_language_pack::get_language("rust").expect("rust grammar");
    let mut old_tree = external_parse(old_source, &rust).expect("parse the old source");
    let inserted = "// added\n".len();
    old_tree.edit(&InputEdit {
        start_byte: 0,
        old_end_byte: 0,
        new_end_byte: inserted,
        start_position: Point::new(0, 0),
        old_end_position: Point::new(0, 0),
        new_end_position: Point::new(1, 0),
    });

    let expected = Processor::new()
        .plan_removals(new_source, path, &resolved_config())
        .expect("plan removals");
    let actual = Processor::new()
        .plan_removals_with(new_source, path, &resolved_config(), |_, parser| {
            parser.parse(new_source, Some(&old_tree))
        })
        .expect("plan removals over an incremental reparse");
    assert_eq!(expected.len(), 2);
    assert_eq!(actual, expected);
}

#[test]
fn a_closure_returning_none_is_the_parse_failure_error() {
    let error = Processor::new()
        .plan_removals_with("fn main() {}\n", Path::new("sample.rs"), &resolved_config(), |_, _| {
            None
        })
        .expect_err("no tree is a parse failure");
    assert_eq!(error.to_string(), "Failed to parse source code");

    let error = Processor::new()
        .inspect_with("x = 1\n", Path::new("sample.py"), &resolved_config(), |_, _| None)
        .expect_err("no tree is a parse failure");
    assert_eq!(error.to_string(), "Failed to parse source code");
}

#[test]
fn an_unsupported_extension_errors_before_the_closure_runs() {
    let mut called = false;
    let result =
        Processor::new().plan_removals_with("noop", Path::new("file.unknownext"), &resolved_config(), |_, _| {
            called = true;
            None
        });
    assert!(result.is_err());
    assert!(!called);
}
