use super::*;

#[test]
fn a_size_alias_shows_the_file_name() {
    let shown = |t: &str| {
        let l = style_line(t, Context::Normal, false);
        l.segs.iter().filter(|g| g.look.link).map(|g| &t[g.range.clone()]).collect::<String>()
    };
    assert_eq!(shown("![[pic.png|300]]"), "pic.png");
    assert_eq!(shown("![[pic.png|300x20]]"), "pic.png");
    assert_eq!(shown("![[pic.png|a caption]]"), "a caption");
    assert_eq!(shown("[[Note|300]]"), "300");
}

#[test]
fn inline_embeds_in_a_line() {
    let found = inline_embeds("see ![[a.png|40]] and ![x](b%20c.png) then ![[N]] end");
    assert_eq!(found, vec![
        ("a.png".to_string(), Some(40), None),
        ("b c.png".to_string(), None, None),
        ("N".to_string(), None, None),
    ]);
    assert!(inline_embeds("no embeds ![ here").is_empty());
}

#[test]
fn standalone_embeds() {
    assert_eq!(standalone_embed("![[a.png]]"), Some(("a.png".into(), None, None)));
    assert_eq!(standalone_embed("  ![[Pics/a.png|300]] "), Some(("Pics/a.png".into(), Some(300), None)));
    assert_eq!(standalone_embed("![[a.png|300x200]]"), Some(("a.png".into(), Some(300), Some(200))));
    assert_eq!(standalone_embed("![cap|120](my%20pic.png)"), Some(("my pic.png".into(), Some(120), None)));
    assert_eq!(standalone_embed("![](https://x.y/a.png)"), None);
    assert_eq!(standalone_embed("see ![[a.png]]"), None);
    assert_eq!(standalone_embed("![[a.png]] and ![[b.png]]"), None);
}

/// The shown text of each segment, `[marker]` for dimmed markup.
fn shown(text: &str, line: &Line) -> Vec<String> {
    line.segs
        .iter()
        .map(|s| {
            let t = &text[s.range.clone()];
            if s.look.marker { format!("[{t}]") } else { t.to_string() }
        })
        .collect()
}

#[test]
fn inactive_lines_hide_markup() {
    let t = "## A **bold** and *it* with `code`";
    let l = style_line(t, Context::Normal, false);
    assert_eq!(l.kind, Kind::Heading(2));
    assert_eq!(shown(t, &l), ["A ", "bold", " and ", "it", " with ", "code"]);
    assert!(l.segs[1].look.bold && l.segs[3].look.italic && l.segs[5].look.code);
    assert_eq!(l.content_start, 3);
}

#[test]
fn the_active_line_shows_markup_dimmed() {
    let t = "## A **bold**";
    let l = style_line(t, Context::Normal, true);
    assert_eq!(shown(t, &l), ["[## ]", "A ", "[**]", "bold", "[**]"]);
    assert!(l.segs[2].look.marker && l.segs[3].look.bold && !l.segs[3].look.marker);
    // Every byte is covered, in order: 1:1 with the source.
    let mut at = 0;
    for s in &l.segs {
        assert_eq!(s.range.start, at);
        at = s.range.end;
    }
    assert_eq!(at, t.len());
}

#[test]
fn links_show_their_alias_and_carry_a_target() {
    let t = "see [[Note#Part|the part]] and [web](https://a.b) or [[Plain]]";
    let l = style_line(t, Context::Normal, false);
    assert_eq!(shown(t, &l), ["see ", "the part", " and ", "web", " or ", "Plain"]);
    assert_eq!(l.links[0], Target::Note { target: "Note".into(), subpath: Some("Part".into()) });
    assert_eq!(l.links[1], Target::Note { target: "Plain".into(), subpath: None });
    assert_eq!(l.links[2], Target::Url("https://a.b".into()));
    assert_eq!(l.segs[1].link, Some(0));
    assert_eq!(l.segs[3].link, Some(2));
}

#[test]
fn frontmatter_lines_get_property_roles() {
    let src = "---\ntitle: \"Hello\"\ntags: [a, \"b c\"]\ndone: true\n\naliases:\n  - one\n  - \"[[Two]]\"\nmeta:\n  nested: x\nempty:\n---\nbody";
    let lines: Vec<String> = src.split('\n').map(String::from).collect();
    let ctx = contexts(&lines);
    let p = properties(&lines, &ctx);
    assert_eq!(p[0], Some(Prop::Open));
    assert_eq!(p[11], Some(Prop::Close));
    assert_eq!(p[12], None);
    assert!(matches!(p[1], Some(Prop::Field { .. })));
    assert_eq!(p[4], Some(Prop::Hidden));
    assert_eq!(p[5], Some(Prop::ListKey));
    assert!(matches!(p[6], Some(Prop::Item { key_line: Some(5), .. })));
    assert!(matches!(p[7], Some(Prop::Item { key_line: None, .. })));
    assert_eq!(p[9], Some(Prop::Raw), "a nested map shows raw");
    assert_eq!(prop_key(&lines[5]), Some("aliases"));

    let shown = |i: usize, key: Option<&str>| {
        let l = style_property(&lines[i], p[i].as_ref().unwrap(), key);
        (l.kind.clone(), l.segs.iter().map(|s| lines[i][s.range.clone()].to_string()).collect::<Vec<_>>(), l)
    };
    let (kind, segs, _) = shown(1, None);
    assert_eq!(kind, Kind::Prop(PropShow::Row { key: Some("title".into()), check: None, empty: false }));
    assert_eq!(segs, ["Hello"], "quotes hide");
    let (_, segs, l) = shown(2, None);
    assert_eq!(segs, ["a", "b c"]);
    assert!(l.segs.iter().all(|s| s.look.pill));
    let (kind, segs, _) = shown(3, None);
    assert!(segs.is_empty());
    assert_eq!(kind, Kind::Prop(PropShow::Row { key: Some("done".into()), check: Some((6..10, true)), empty: false }));
    let (kind, segs, _) = shown(6, Some("aliases"));
    assert_eq!(segs, ["one"]);
    assert!(matches!(kind, Kind::Prop(PropShow::Row { key: Some(k), .. }) if k == "aliases"));
    let (_, segs, l) = shown(7, None);
    assert_eq!(segs, ["Two"], "a link item shows its name");
    assert_eq!(l.links, [Target::Note { target: "Two".into(), subpath: None }]);
    let (kind, _, _) = shown(10, None);
    assert_eq!(kind, Kind::Prop(PropShow::Row { key: Some("empty".into()), check: None, empty: true }));

    // Never closed: no table.
    let open: Vec<String> = ["---", "a: b"].iter().map(|s| s.to_string()).collect();
    assert!(properties(&open, &contexts(&open)).iter().all(Option::is_none));
}

#[test]
fn fenced_blocks_are_found_and_fences_hide() {
    let lines: Vec<String> = ["a", "```", "x", "```", "```", "y"].iter().map(|s| s.to_string()).collect();
    let ctx = contexts(&lines);
    assert_eq!(fenced_block(&ctx, 0), None);
    assert_eq!(fenced_block(&ctx, 2), Some((1, 3)));
    assert_eq!(fenced_block(&ctx, 3), Some((1, 3)));
    assert_eq!(fenced_block(&ctx, 4), Some((4, 5)));
    assert!(style_line("```rust", Context::Fence, false).segs.is_empty());
    assert_eq!(style_line("```rust", Context::Fence, true).segs.len(), 1);
}

#[test]
fn lists_tasks_quotes_and_rules() {
    let t = "\t- [x] done *thing*";
    let l = style_line(t, Context::Normal, false);
    assert_eq!(l.kind, Kind::List { level: 1, marker: Marker::Task('x', 4) });
    assert_eq!(shown(t, &l), ["done ", "thing"]);
    assert!(l.segs[0].look.dim);
    let l = style_line("1. first", Context::Normal, false);
    assert_eq!(l.kind, Kind::List { level: 0, marker: Marker::Number("1.".into()) });
    let t = "> > quoted";
    let l = style_line(t, Context::Normal, false);
    assert_eq!((l.kind.clone(), shown(t, &l)), (Kind::Quote(2), vec!["quoted".to_string()]));
    assert_eq!(style_line("---", Context::Normal, false).kind, Kind::Rule);
    assert!(style_line("---", Context::Normal, false).segs.is_empty());
    assert_eq!(style_line("- ", Context::Normal, false).kind, Kind::List { level: 0, marker: Marker::Bullet });
}

#[test]
fn emphasis_needs_real_pairs() {
    let t = "a * b * c snake_case_word 2*3*4 ==hi== ~~no~~";
    let l = style_line(t, Context::Normal, false);
    let styled: Vec<(String, bool, bool, bool)> = l
        .segs
        .iter()
        .map(|s| (t[s.range.clone()].to_string(), s.look.italic, s.look.highlight, s.look.strike))
        .collect();
    assert!(styled.iter().any(|(s, _, h, _)| s == "hi" && *h));
    assert!(styled.iter().any(|(s, _, _, k)| s == "no" && *k));
    assert!(styled.iter().all(|(s, i, _, _)| !(s.contains("snake") && *i)), "{styled:?}");
    assert!(!styled.iter().any(|(s, i, _, _)| s.contains(" b ") && *i), "spaced * is not emphasis");
}

#[test]
fn tags_and_code_protect() {
    let t = "#tag and `#not **x**` #2026 #a/b";
    let l = style_line(t, Context::Normal, false);
    let tags: Vec<&str> = l.segs.iter().filter(|s| s.look.tag).map(|s| &t[s.range.clone()]).collect();
    assert_eq!(tags, ["#tag", "#a/b"]);
    assert!(l.segs.iter().any(|s| s.look.code && &t[s.range.clone()] == "#not **x**"));
}

#[test]
fn block_contexts() {
    let lines: Vec<String> = ["---", "a: 1", "---", "text", "```rust", "let x;", "```", "after"].iter().map(|s| s.to_string()).collect();
    assert_eq!(
        contexts(&lines),
        [
            Context::Frontmatter,
            Context::Frontmatter,
            Context::Frontmatter,
            Context::Normal,
            Context::Fence,
            Context::Code,
            Context::Fence,
            Context::Normal
        ]
    );
    assert!(style_line("let x;", Context::Code, false).segs[0].look.mono);
}
