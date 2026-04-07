use devin::parser::{parse_file, CodeChunk};

#[test]
fn test_rust_fn_chunks() {
    let source = r#"
pub fn foo(x: u32) -> u32 {
    x + 1
}

fn bar() {
    println!("hi");
}
"#;
    let chunks = parse_file("src/lib.rs", source);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].name, "foo");
    assert_eq!(chunks[0].kind, "fn");
    assert_eq!(chunks[1].name, "bar");
    assert_eq!(chunks[1].kind, "fn");
}

#[test]
fn test_rust_struct_and_impl() {
    let source = r#"
pub struct Foo {
    x: u32,
}

impl Foo {
    pub fn new() -> Self { Foo { x: 0 } }
}
"#;
    let chunks = parse_file("src/foo.rs", source);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].name, "Foo");
    assert_eq!(chunks[0].kind, "struct");
    assert_eq!(chunks[1].name, "Foo");
    assert_eq!(chunks[1].kind, "impl");
}

#[test]
fn test_rust_enum_and_trait() {
    let source = r#"
pub enum Color { Red, Green }

pub trait Draw {
    fn draw(&self);
}
"#;
    let chunks = parse_file("src/traits.rs", source);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].kind, "enum");
    assert_eq!(chunks[1].kind, "trait");
}

#[test]
fn test_python_def_and_class() {
    let source = r#"
def compute(x):
    return x * 2

class MyModel:
    def __init__(self):
        self.x = 0
"#;
    let chunks = parse_file("model.py", source);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].name, "compute");
    assert_eq!(chunks[0].kind, "def");
    assert_eq!(chunks[1].name, "MyModel");
    assert_eq!(chunks[1].kind, "class");
}

#[test]
fn test_unknown_extension_single_chunk() {
    let source = "hello world\nsome text\n";
    let chunks = parse_file("README.md", source);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].kind, "file");
    assert_eq!(chunks[0].name, "README.md");
}

#[test]
fn test_start_end_lines() {
    let source = "fn foo() {\n    1\n}\n\nfn bar() {\n    2\n}\n";
    let chunks = parse_file("src/lib.rs", source);
    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].start_line < chunks[1].start_line);
    assert!(chunks[0].end_line < chunks[1].start_line);
}

#[test]
fn test_chunk_body_contains_source() {
    let source = "fn foo() -> u32 {\n    42\n}\n";
    let chunks = parse_file("src/lib.rs", source);
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].body.contains("42"));
}
