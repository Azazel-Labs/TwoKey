//! Syntax-aware extraction: named symbols (functions, types, methods, ...) with
//! qualified names, plus fingerprints of arbitrary tree-sitter query captures.
//!
//! Fingerprints are whitespace-insensitive but otherwise exact: every leaf token's
//! text is kept (comments included), and any non-whitespace source text that no
//! leaf covers is kept too, so a grammar that hides tokens can't hide a change.

use crate::lang::Lang;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

#[derive(Debug, Clone)]
pub struct Symbol {
    pub qname: String,
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct Capture {
    pub line: usize,
    pub fingerprint: String,
    pub preview: String,
}

pub fn extract(lang: Lang, tree: &Tree, src: &[u8]) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut scope: Vec<String> = Vec::new();
    visit(lang, tree.root_node(), src, &mut scope, &mut out, 0);
    out
}

fn text<'a>(n: Node, src: &'a [u8]) -> &'a str {
    std::str::from_utf8(&src[n.start_byte()..n.end_byte()]).unwrap_or("")
}

fn field_text(n: Node, field: &str, src: &[u8]) -> Option<String> {
    n.child_by_field_name(field).map(|c| text(c, src).to_string())
}

/// Strip generics / pointer / reference sugar from a type used as a scope name.
fn clean_type(t: &str) -> String {
    let t = t.trim().trim_start_matches(['&', '*']).trim();
    let t = t.trim_start_matches("mut ").trim_start_matches("dyn ").trim();
    let cut = t.find(['<', '[', '(']).unwrap_or(t.len());
    t[..cut].trim().to_string()
}

struct Def<'t> {
    name: String,
    kind: &'static str,
    container: bool,
    /// Nodes whose combined span is fingerprinted (e.g. attributes + item).
    span: Vec<Node<'t>>,
    /// Explicit scope override (Go methods).
    scope_override: Option<String>,
}

fn def_info<'t>(lang: Lang, n: Node<'t>, src: &[u8]) -> Option<Def<'t>> {
    let kind = n.kind();
    let simple = |k: &'static str, container: bool| -> Option<Def<'t>> {
        Some(Def {
            name: field_text(n, "name", src)?,
            kind: k,
            container,
            span: vec![n],
            scope_override: None,
        })
    };
    match lang {
        Lang::Rust => {
            let mut d = match kind {
                "function_item" | "function_signature_item" => simple("fn", true)?,
                "struct_item" => simple("struct", false)?,
                "enum_item" => simple("enum", false)?,
                "union_item" => simple("union", false)?,
                "type_item" => simple("type", false)?,
                "const_item" => simple("const", false)?,
                "static_item" => simple("static", false)?,
                "macro_definition" => simple("macro", false)?,
                "trait_item" => simple("trait", true)?,
                "mod_item" => simple("mod", true)?,
                _ => return None,
            };
            // Attributes (#[...]) are preceding siblings; include them so that
            // e.g. removing #[require_auth] or adding #[ignore] counts as a change.
            // Comments between attributes and the item don't end the walk.
            extend_back(&mut d, n, &["attribute_item"], &["line_comment", "block_comment"]);
            Some(d)
        }
        Lang::Python => {
            let mut d = match kind {
                "function_definition" => simple("def", true)?,
                "class_definition" => simple("class", true)?,
                _ => return None,
            };
            if let Some(p) = n.parent()
                && p.kind() == "decorated_definition"
            {
                d.span = vec![p];
            }
            Some(d)
        }
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => {
            let mut d = match kind {
                "function_declaration" | "generator_function_declaration" | "function_signature" => {
                    simple("function", true)?
                }
                "class_declaration" | "abstract_class_declaration" => simple("class", true)?,
                "interface_declaration" => simple("interface", true)?,
                "type_alias_declaration" => simple("type", false)?,
                "enum_declaration" => simple("enum", false)?,
                "method_definition" | "method_signature" | "abstract_method_signature" => {
                    let mut d = simple("method", true)?;
                    // In TS, method decorators are preceding siblings in class_body.
                    extend_back(&mut d, n, &["decorator"], &["comment"]);
                    d
                }
                "variable_declarator" => {
                    let v = n.child_by_field_name("value")?;
                    if !matches!(
                        v.kind(),
                        "arrow_function" | "function_expression" | "function" | "class" | "generator_function"
                    ) {
                        return None;
                    }
                    let mut d = simple("function", true)?;
                    // Include the `const`/`let` declaration so const→let is visible.
                    if let Some(decl) = n.parent() {
                        d.span = vec![decl];
                    }
                    d
                }
                _ => return None,
            };
            // Include `export` (and decorators written before it) so dropping or
            // changing the export is a change to the symbol.
            let top = *d.span.last().unwrap_or(&n);
            if let Some(p) = top.parent()
                && p.kind() == "export_statement"
            {
                d.span = vec![p];
            }
            Some(d)
        }
        // Annotations (Java `modifiers`) and attributes (C# `attribute_list`) are
        // children of the declaration, so they're already inside its span.
        Lang::Java => match kind {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => simple("class", true),
            "method_declaration" | "constructor_declaration" | "compact_constructor_declaration" => {
                simple("method", true)
            }
            _ => None,
        },
        Lang::CSharp => match kind {
            "class_declaration" | "struct_declaration" | "interface_declaration" | "record_declaration" => {
                simple("class", true)
            }
            "enum_declaration" => simple("enum", false),
            // (newer grammars nest declarations inside file-scoped namespaces;
            // older ones make them siblings, handled in `visit`)
            "namespace_declaration" | "file_scoped_namespace_declaration" => simple("namespace", true),
            "method_declaration" | "constructor_declaration" | "local_function_statement" => simple("method", true),
            "property_declaration" => simple("property", true),
            _ => None,
        },
        Lang::Ruby => match kind {
            "class" | "module" => simple(if kind == "class" { "class" } else { "module" }, true),
            "method" => simple("def", true),
            // `def self.verify` must not collide with an instance `def verify`.
            "singleton_method" => {
                let mut d = simple("def", true)?;
                d.name = format!("self.{}", d.name);
                Some(d)
            }
            _ => None,
        },
        Lang::Go => match kind {
            "function_declaration" => simple("func", false),
            "type_spec" | "type_alias" => simple("type", false),
            "method_declaration" => {
                let mut d = simple("method", false)?;
                let recv = n.child_by_field_name("receiver")?;
                let mut c = recv.walk();
                let ty = recv
                    .named_children(&mut c)
                    .find_map(|p| p.child_by_field_name("type"))
                    .map(|t| clean_type(text(t, src)));
                d.scope_override = ty;
                Some(d)
            }
            _ => None,
        },
    }
}

/// Extend a definition's span backwards over preceding siblings of kind `take`
/// (attributes, decorators), skipping over `skip` kinds (comments) in between.
/// The span starts at the earliest `take` sibling found.
fn extend_back<'t>(d: &mut Def<'t>, n: Node<'t>, take: &[&str], skip: &[&str]) {
    let mut prev = n.prev_sibling();
    let mut earliest: Option<Node<'t>> = None;
    while let Some(p) = prev {
        if take.contains(&p.kind()) {
            earliest = Some(p);
        } else if !skip.contains(&p.kind()) {
            break;
        }
        prev = p.prev_sibling();
    }
    if let Some(e) = earliest {
        // Span is contiguous siblings from `e` to the item.
        d.span = vec![e, *d.span.last().unwrap_or(&n)];
    }
}

/// Rust `impl Foo { ... }` / `impl Trait for Foo` contribute `Foo` as a scope.
fn scope_only(lang: Lang, n: Node, src: &[u8]) -> Option<String> {
    match (lang, n.kind()) {
        (Lang::Rust, "impl_item") => n.child_by_field_name("type").map(|t| clean_type(text(t, src))),
        _ => None,
    }
}

fn visit(lang: Lang, n: Node, src: &[u8], scope: &mut Vec<String>, out: &mut Vec<Symbol>, depth: usize) {
    if depth > 512 {
        return;
    }
    let mut pushed = false;
    if let Some(d) = def_info(lang, n, src) {
        let sep = lang.sep();
        let mut parts: Vec<String> = match &d.scope_override {
            Some(s) => vec![s.clone()],
            None => scope.clone(),
        };
        parts.push(d.name.clone());
        let qname = parts.join(sep);
        let start = d.span.first().copied().unwrap_or(n);
        let end = d.span.last().copied().unwrap_or(n);
        out.push(Symbol {
            qname,
            name: d.name.trim_start_matches("self.").to_string(),
            kind: d.kind,
            line: n.start_position().row + 1,
            fingerprint: fingerprint_span(start, end, src),
        });
        if d.container {
            scope.push(d.name);
            pushed = true;
        }
    } else if let Some(s) = scope_only(lang, n, src) {
        scope.push(s);
        pushed = true;
    }
    let mut c = n.walk();
    let kids: Vec<Node> = n.children(&mut c).collect();
    // C# `namespace App;` applies to the declarations that follow it as siblings.
    let mut file_ns = 0;
    for k in kids {
        visit(lang, k, src, scope, out, depth + 1);
        if lang == Lang::CSharp
            && k.kind() == "file_scoped_namespace_declaration"
            && k.named_child_count() <= 1
            && let Some(name) = field_text(k, "name", src)
        {
            scope.push(name);
            file_ns += 1;
        }
    }
    for _ in 0..file_ns {
        scope.pop();
    }
    if pushed {
        scope.pop();
    }
}

/// Fingerprint the source span from `start` to `end` (sibling nodes).
///
/// The fingerprint is structural: named nodes contribute `kind(` ... `)` around
/// their leaves. Whitespace is ignored, but anything whitespace *means* to the
/// grammar is kept: re-indenting a Python statement out of an `if` block, or a
/// line break that triggers JS automatic semicolon insertion, changes the tree
/// and therefore the fingerprint.
pub fn fingerprint_span(start: Node, end: Node, src: &[u8]) -> String {
    let (s, e) = (start.start_byte(), end.end_byte());
    let mut events: Vec<Ev> = Vec::new();
    let mut n = Some(start);
    while let Some(node) = n {
        collect_events(node, &mut events);
        if node.id() == end.id() || node.start_byte() >= e {
            break;
        }
        n = node.next_sibling();
    }
    fingerprint_events(&events, s, e, src)
}

pub fn fingerprint_node(n: Node, src: &[u8]) -> String {
    fingerprint_span(n, n, src)
}

enum Ev<'t> {
    Open(&'t str),
    Close,
    Leaf(Node<'t>),
}

fn collect_events<'t>(n: Node<'t>, out: &mut Vec<Ev<'t>>) {
    enum Step<'t> {
        Enter(Node<'t>),
        Exit,
    }
    let mut stack = vec![Step::Enter(n)];
    while let Some(step) = stack.pop() {
        let x = match step {
            Step::Exit => {
                out.push(Ev::Close);
                continue;
            }
            Step::Enter(x) => x,
        };
        if x.child_count() == 0 {
            out.push(Ev::Leaf(x));
            continue;
        }
        if x.is_named() {
            out.push(Ev::Open(x.kind()));
            stack.push(Step::Exit);
        }
        let mut c = x.walk();
        let kids: Vec<Node> = x.children(&mut c).collect();
        for k in kids.into_iter().rev() {
            stack.push(Step::Enter(k));
        }
    }
}

fn fingerprint_events(events: &[Ev], start: usize, end: usize, src: &[u8]) -> String {
    let mut fp = String::new();
    let mut pos = start;
    // Non-whitespace text no leaf covers is kept, so a grammar that hides tokens
    // can't hide a change.
    let push_gap = |fp: &mut String, a: usize, b: usize| {
        if b > a {
            let gap = String::from_utf8_lossy(&src[a..b]);
            let g = gap.split_whitespace().collect::<Vec<_>>().join(" ");
            if !g.is_empty() {
                fp.push_str(&g);
                fp.push('\u{1f}');
            }
        }
    };
    for ev in events {
        match ev {
            Ev::Open(k) => {
                fp.push_str(k);
                fp.push('(');
            }
            Ev::Close => fp.push(')'),
            Ev::Leaf(l) => {
                let (ls, le) = (l.start_byte().max(pos), l.end_byte().min(end));
                push_gap(&mut fp, pos, ls.min(end));
                if l.is_missing() {
                    fp.push_str("<missing:");
                    fp.push_str(l.kind());
                    fp.push('>');
                } else if le > ls {
                    fp.push_str(&String::from_utf8_lossy(&src[ls..le]));
                }
                fp.push('\u{1f}');
                pos = pos.max(le);
            }
        }
    }
    push_gap(&mut fp, pos, end);
    fp
}

/// Run a compiled query and fingerprint every node captured as `capture_idx`.
pub fn captures(query: &Query, capture_idx: u32, tree: &Tree, src: &[u8]) -> Vec<Capture> {
    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut matches = cursor.matches(query, tree.root_node(), src);
    while let Some(m) = matches.next() {
        for cap in m.captures() {
            if cap.index != capture_idx || !seen.insert(cap.node.id()) {
                continue;
            }
            let n = cap.node;
            let preview: String = text(n, src)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .chars()
                .take(70)
                .collect();
            out.push(Capture {
                line: n.start_position().row + 1,
                fingerprint: fingerprint_node(n, src),
                preview,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::parse;

    fn syms(lang: Lang, src: &str) -> Vec<(String, String)> {
        let t = parse(lang, src.as_bytes()).unwrap();
        extract(lang, &t, src.as_bytes())
            .into_iter()
            .map(|s| (s.qname, s.fingerprint))
            .collect()
    }

    fn names(lang: Lang, src: &str) -> Vec<String> {
        syms(lang, src).into_iter().map(|s| s.0).collect()
    }

    #[test]
    fn rust_qualified_names() {
        let n = names(
            Lang::Rust,
            "mod auth { pub struct Session; impl Session { pub fn verify(&self) -> bool { true } } }\nfn top() {}\nimpl<T> Trait for Wrapper<T> { fn go(&self) {} }",
        );
        assert!(n.contains(&"auth".to_string()));
        assert!(n.contains(&"auth::Session".to_string()));
        assert!(n.contains(&"auth::Session::verify".to_string()));
        assert!(n.contains(&"top".to_string()));
        assert!(n.contains(&"Wrapper::go".to_string()));
    }

    #[test]
    fn whitespace_insensitive_but_token_exact() {
        let a = syms(Lang::Rust, "fn f() -> u8 { 1 }");
        let b = syms(Lang::Rust, "fn f()\n    -> u8\n{\n    1\n}\n");
        let c = syms(Lang::Rust, "fn f() -> u8 { 2 }");
        assert_eq!(a[0].1, b[0].1);
        assert_ne!(a[0].1, c[0].1);
    }

    #[test]
    fn rust_attributes_are_part_of_symbol() {
        let a = syms(Lang::Rust, "#[require_auth]\nfn handler() {}");
        let b = syms(Lang::Rust, "fn handler() {}");
        assert_ne!(a[0].1, b[0].1);
    }

    #[test]
    fn python_decorators_and_classes() {
        let src = "class Session:\n    @require_auth\n    def verify(self):\n        return True\n";
        let n = names(Lang::Python, src);
        assert!(n.contains(&"Session.verify".to_string()));
        let a = syms(Lang::Python, src);
        let b = syms(
            Lang::Python,
            "class Session:\n    def verify(self):\n        return True\n",
        );
        let fa = a.iter().find(|s| s.0 == "Session.verify").unwrap();
        let fb = b.iter().find(|s| s.0 == "Session.verify").unwrap();
        assert_ne!(fa.1, fb.1);
    }

    #[test]
    fn meaningful_whitespace_counts() {
        let a = syms(
            Lang::Python,
            "def v(t):\n    if t == 1:\n        return True\n    return False\n",
        );
        let b = syms(
            Lang::Python,
            "def v(t):\n    if t == 1:\n        pass\n    return True\n    return False\n",
        );
        let c = syms(
            Lang::Python,
            "def v(t):\n    if t == 1:\n        pass\n        return True\n    return False\n",
        );
        assert_ne!(b[0].1, c[0].1, "dedent out of the if block must change the fingerprint");
        assert_ne!(a[0].1, b[0].1);
        let j1 = syms(Lang::JavaScript, "function v(t) { return isValid(t) }");
        let j2 = syms(Lang::JavaScript, "function v(t) { return\nisValid(t) }");
        assert_ne!(j1[0].1, j2[0].1, "ASI after return must change the fingerprint");
    }

    #[test]
    fn rust_attribute_behind_comment_and_ts_decorators() {
        let a = syms(Lang::Rust, "#[require_auth]\n// note\nfn h() {}");
        let b = syms(Lang::Rust, "// note\nfn h() {}");
        assert_ne!(a[0].1, b[0].1);
        let t1 = syms(
            Lang::TypeScript,
            "class A {\n  @RequireAuth()\n  verify() { return true }\n}",
        );
        let t2 = syms(Lang::TypeScript, "class A {\n  verify() { return true }\n}");
        let f1 = t1.iter().find(|s| s.0 == "A.verify").unwrap();
        let f2 = t2.iter().find(|s| s.0 == "A.verify").unwrap();
        assert_ne!(f1.1, f2.1);
    }

    #[test]
    fn java_csharp_ruby() {
        let j = syms(
            Lang::Java,
            "class Auth {\n  @RequiresRole(\"admin\")\n  boolean verify(String t) { return true; }\n}",
        );
        let j2 = syms(
            Lang::Java,
            "class Auth {\n  boolean verify(String t) { return true; }\n}",
        );
        let a = j.iter().find(|s| s.0 == "Auth.verify").unwrap();
        let b = j2.iter().find(|s| s.0 == "Auth.verify").unwrap();
        assert_ne!(a.1, b.1, "removing an annotation is a change");
        let c = names(
            Lang::CSharp,
            "namespace App { class Auth { [Authorize] public bool Verify() { return true; } } }",
        );
        assert!(c.contains(&"App.Auth.Verify".to_string()), "{c:?}");
        let r = names(
            Lang::Ruby,
            "module Auth\n  class Session\n    def verify(t)\n      true\n    end\n    def self.verify(t)\n      false\n    end\n  end\nend\n",
        );
        assert!(r.contains(&"Auth::Session::verify".to_string()), "{r:?}");
        assert!(r.contains(&"Auth::Session::self.verify".to_string()), "{r:?}");
        let f = names(
            Lang::CSharp,
            "namespace App;\nclass Auth { public bool Verify() { return true; } }\n",
        );
        assert!(f.contains(&"App.Auth.Verify".to_string()), "{f:?}");
    }

    #[test]
    fn ts_and_go() {
        let n = names(
            Lang::TypeScript,
            "export class Auth { verify(t: string): boolean { return true } }\nexport const check = (x: number) => x > 0;",
        );
        assert!(n.contains(&"Auth.verify".to_string()));
        assert!(n.contains(&"check".to_string()));
        let g = names(
            Lang::Go,
            "package a\nfunc (s *Session) Verify() bool { return true }\nfunc Top() {}",
        );
        assert!(g.contains(&"Session.Verify".to_string()));
        assert!(g.contains(&"Top".to_string()));
    }
}
