use anyhow::Context;
use indicatif::ParallelProgressIterator;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::fmt::{Debug, Formatter};
use std::iter::Sum;
use std::ops::Add;
use std::path::PathBuf;
use syn::__private::ToTokens;
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream};
use syn::visit::Visit;
use syn::{Expr, ExprMacro, Ident, Token};

struct MacroCallParser {
    exprs: Vec<Expr>,
}

impl Debug for MacroCallParser {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for expr in &self.exprs {
            write!(f, "{:?}, ", expr.to_token_stream().to_string())?;
        }
        Ok(())
    }
}

impl Parse for MacroCallParser {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let first: Expr = input.parse()?;
        let mut exprs = vec![first];
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            if input.peek(Ident::peek_any) && input.peek2(Token![=]) {
                let _name: Ident = input.call(Ident::parse_any)?;
                input.parse::<Token![=]>()?;
                let value: Expr = input.parse()?;
                exprs.push(value);
            } else {
                exprs.push(input.parse()?);
            }
        }

        Ok(Self { exprs })
    }
}

#[derive(Debug, Default, Copy, Clone)]
struct ArgsCounter {
    simple_field_access: u64,
    nested_field_access: u64,
    ident: u64,
    method_call: u64,
    other: u64,
}

impl Add for ArgsCounter {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            simple_field_access: self.simple_field_access + rhs.simple_field_access,
            nested_field_access: self.nested_field_access + rhs.nested_field_access,
            ident: self.ident + rhs.ident,
            method_call: self.method_call + rhs.method_call,
            other: self.other + rhs.other,
        }
    }
}

impl Sum for ArgsCounter {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), Add::add)
    }
}

#[derive(Debug)]
struct MacroCall {
    name: String,
    counter: ArgsCounter,
}

#[derive(Default, Debug)]
struct MacroFinder {
    macros: Vec<MacroCall>,
}

impl syn::visit::Visit<'_> for MacroFinder {
    fn visit_expr_macro(&mut self, makro: &'_ ExprMacro) {
        let Some(ident) = makro.mac.path.get_ident() else {
            return;
        };
        let ident = ident.to_string();
        let args_start_index = match ident.as_str() {
            "format_args" | "format" | "panic" | "unreachable" | "unimplemented" | "todo"
            | "info" | "debug" | "warn" | "error" | "trace" | "print" | "println" | "eprint"
            | "eprintln" => 1,
            "write" | "writeln" | "assert" => 2,
            "assert_eq" | "assert_ne" => 3,
            _ => return,
        };

        let Ok(call) = makro.mac.parse_body::<MacroCallParser>() else {
            return;
        };
        if call.exprs.len() <= args_start_index {
            return;
        }

        let mut counter = ArgsCounter::default();

        fn is_ident(expr: &Expr) -> bool {
            match expr {
                Expr::Path(path) if path.qself.is_none() && path.path.get_ident().is_some() => true,
                _ => false,
            }
        }

        fn strip_field_accesses(expr: &Expr) -> &Expr {
            match expr {
                Expr::Field(field) => strip_field_accesses(&field.base),
                _ => expr,
            }
        }

        fn is_simple_method_call(expr: &Expr) -> bool {
            match expr {
                Expr::MethodCall(method) => method.turbofish.is_none() && method.args.is_empty(),
                _ => false,
            }
        }

        for expr in &call.exprs[args_start_index..] {
            if is_ident(expr) {
                counter.ident += 1;
            } else if let Expr::Field(field) = expr {
                if is_ident(&field.base) {
                    counter.simple_field_access += 1;
                } else {
                    let leaf = strip_field_accesses(&field.base);
                    if is_ident(leaf) {
                        counter.nested_field_access += 1;
                    }
                }
            } else if let Expr::MethodCall(_) = expr {
                counter.method_call += 1;
            } else {
                counter.other += 1;
            }
        }

        self.macros.push(MacroCall {
            name: ident,
            counter,
        });
    }
}

fn main() -> anyhow::Result<()> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .stack_size(128 * 1024 * 1024)
        .build_global()
        .unwrap();

    // TODO: most starred GitHub Rust repos
    let dir = PathBuf::from("crates");
    // download_top_n_crates(&dir, 10000)?;
    // download_git_repo(&dir, "rust-lang", "rust")?;

    let files = glob::glob(&format!("{}/**/*.rs", dir.display()))?
        .into_iter()
        .filter_map(|path| path.ok())
        .collect::<Vec<PathBuf>>();
    eprintln!("Found {} files", files.len());

    let calls = files
        .par_iter()
        .progress_count(files.len() as u64)
        .map(|path| {
            let src = std::fs::read_to_string(&path)?;
            let module = syn::parse_file(&src).context(path.to_str().unwrap().to_string())?;
            let mut finder = MacroFinder::default();
            finder.visit_file(&module);
            anyhow::Ok(finder.macros)
        })
        .filter_map(|r| r.ok())
        .flatten()
        .collect::<Vec<MacroCall>>();

    eprintln!("Found {} macro calls", calls.len());

    let counter: ArgsCounter = calls.iter().map(|c| c.counter).sum();
    eprintln!("Ident: {}", counter.ident);
    eprintln!("Field access: {}", counter.simple_field_access);
    eprintln!("Nested field access: {}", counter.nested_field_access);
    eprintln!("Method call: {}", counter.method_call);
    eprintln!("Other: {}", counter.other);

    let can_be_inlined_now = calls
        .iter()
        .filter(|c| {
            c.counter.other == 0
                && c.counter.method_call == 0
                && c.counter.simple_field_access == 0
                && c.counter.nested_field_access == 0
        })
        .count();
    eprintln!("Inlineable today: {}/{}", can_be_inlined_now, calls.len());

    let can_be_inlined_after = calls
        .iter()
        .filter(|c| c.counter.other == 0 && c.counter.method_call == 0)
        .count();
    eprintln!(
        "Inlineable if we support field accesses: {}/{}",
        can_be_inlined_after,
        calls.len()
    );

    Ok(())
}
