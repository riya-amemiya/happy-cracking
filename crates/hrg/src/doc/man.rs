use std::fmt::Write;

use super::{Category, FlagDoc, help, rename, render_markup, rg_flags};

const TEMPLATE: &str = include_str!("template.1");

pub(crate) fn man_page(prog: &str) -> String {
    render(prog, prog)
}

pub(super) fn render(prog: &str, tool: &str) -> String {
    let page = help::template(TEMPLATE, prog).replace("!!PROG_UPPER!!", &prog.to_uppercase());
    Category::ALL.into_iter().fold(page, |out, cat| {
        let body: Vec<String> = rg_flags()
            .filter(|flag| flag.cat == cat)
            .map(|flag| flag_section(flag, prog, tool))
            .collect();
        out.replace(cat.placeholder(), &body.join(".sp\n"))
    })
}

fn escape(name: &str) -> String {
    name.replace('-', r"\-")
}

fn flag_section(flag: &FlagDoc, prog: &str, tool: &str) -> String {
    let mut out = String::new();
    if let Some(short) = flag.short {
        write!(out, r"\fB\-{short}\fP").unwrap();
        if let Some(var) = flag.var {
            write!(out, r" \fI{var}\fP").unwrap();
        }
        if flag.long.is_some() {
            out.push_str(", ");
        }
    }
    if let Some(long) = flag.long {
        write!(out, r"\fB\-\-{}\fP", escape(long)).unwrap();
        if let Some(var) = flag.var {
            write!(out, r"=\fI{var}\fP").unwrap();
        }
    }
    out.push_str("\n.RS 4\n");

    let doc = rename(flag.long_doc.trim(), prog, tool);
    let doc = render_markup(&doc, "flag", |flag, out| {
        out.push_str(r"\fB");
        if let Some(short) = flag.short {
            write!(out, r"\-{short}/").unwrap();
        }
        write!(out, r"\-\-{}\fP", escape(flag.long.unwrap_or_default())).unwrap();
    });
    let doc = render_markup(&doc, "flag-negate", |flag, out| {
        write!(out, r"\fB\-\-{}\fP", flag.negated.unwrap_or_default()).unwrap();
    });
    writeln!(out, "{doc}").unwrap();
    if let Some(negated) = flag.negated
        && flag.is_switch()
    {
        writeln!(
            out,
            ".sp\nThis flag can be disabled with \\fB\\-\\-{negated}\\fP."
        )
        .unwrap();
    }
    out.push_str(".RE\n");
    out
}
