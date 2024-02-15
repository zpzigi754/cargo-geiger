use super::{
    file_forbids_unsafe, has_unsafe_attributes, is_test_fn, is_test_mod,
    IncludeTests, RsFileMetrics,
};

use quote::quote;
use syn::{
    visit, Expr, ExprUnary, ExprUnsafe, ImplItemMethod, ItemFn, ItemImpl,
    ItemMod, ItemTrait, UnOp,
};

pub struct GeigerSynVisitor {
    /// Count unsafe usage inside tests
    include_tests: IncludeTests,

    /// The resulting data from a single file scan.
    pub metrics: RsFileMetrics,

    /// The number of nested unsafe scopes that the GeigerSynVisitor are
    /// currently in. For example, if the visitor is inside an unsafe function
    /// and inside an unnecessary unsafe block inside that function, then this
    /// number should be 2. If the visitor is outside unsafe scopes, in a safe
    /// scope, this number should be 0.
    /// This is needed since unsafe scopes can be nested and we need to know
    /// when we leave the outmost unsafe scope and get back into a safe scope.
    unsafe_scopes: u32,

    has_unsafe_deref: bool,
}

impl GeigerSynVisitor {
    pub fn new(include_tests: IncludeTests) -> Self {
        GeigerSynVisitor {
            include_tests,
            metrics: Default::default(),
            unsafe_scopes: 0,
            has_unsafe_deref: false,
        }
    }

    pub fn enter_unsafe_scope(&mut self) {
        self.unsafe_scopes += 1;
    }

    pub fn exit_unsafe_scope(&mut self) {
        self.unsafe_scopes -= 1;
    }
}

impl<'ast> visit::Visit<'ast> for GeigerSynVisitor {
    fn visit_file(&mut self, i: &'ast syn::File) {
        self.metrics.forbids_unsafe = file_forbids_unsafe(i);
        syn::visit::visit_file(self, i);
    }

    /// Free-standing functions
    fn visit_item_fn(&mut self, item_fn: &ItemFn) {
        if IncludeTests::No == self.include_tests && is_test_fn(item_fn) {
            return;
        }
        let unsafe_fn =
            item_fn.sig.unsafety.is_some() || has_unsafe_attributes(item_fn);
        let block_start = self.metrics.counters.exprs.unsafe_;
        if unsafe_fn {
            if item_fn.sig.unsafety.is_some() {
                print!("{}", quote!(#item_fn).to_string());
            }
            self.enter_unsafe_scope()
        }
        self.metrics.counters.functions.count(unsafe_fn);
        visit::visit_item_fn(self, item_fn);
        if item_fn.sig.unsafety.is_some() {
            let block_end = self.metrics.counters.exprs.unsafe_;
            print!(
                " - stmt: {}, expr: {} ",
                &item_fn.block.stmts.len(),
                block_end - block_start
            );

            if self.has_unsafe_deref {
                println!("(Dereference Operation: Unsafe Function)");
                self.has_unsafe_deref = false;
            } else {
                println!("");
            }

            self.exit_unsafe_scope()
        }
    }

    fn visit_expr(&mut self, i: &Expr) {
        // Total number of expressions of any type
        match i {
            Expr::Unsafe(i) => {
                self.enter_unsafe_scope();
                self.visit_expr_unsafe(i);
                self.exit_unsafe_scope();
            }
            Expr::Path(_) | Expr::Lit(_) => {
                // Do not count. The expression `f(x)` should count as one
                // expression, not three.
            }
            other => {
                // TODO: Print something pretty here or gather the data for later
                // printing.
                // if self.verbosity == Verbosity::Verbose && self.unsafe_scopes > 0 {
                //     println!("{:#?}", other);
                // }
                self.metrics.counters.exprs.count(self.unsafe_scopes > 0);
                visit::visit_expr(self, other);
            }
        }
    }

    fn visit_expr_unsafe(&mut self, i: &ExprUnsafe) {
        print!("{}", quote!(#i).to_string());
        let block_start = self.metrics.counters.exprs.unsafe_;
        for stmt in &i.block.stmts {
            self.visit_stmt(stmt);
        }
        let block_end = self.metrics.counters.exprs.unsafe_;
        print!(
            " - stmt: {}, expr: {} ",
            &i.block.stmts.len(),
            block_end - block_start
        );

        if self.has_unsafe_deref {
            println!("(Dereference Operation: Unsafe Block)");
            self.has_unsafe_deref = false;
        } else {
            println!("");
        }
    }

    fn visit_expr_unary(&mut self, i: &ExprUnary) {
        if self.unsafe_scopes > 0 {
            if let UnOp::Deref(_) = i.op {
                self.has_unsafe_deref = true;
            }
        }
        visit::visit_expr_unary(self, i);
    }

    fn visit_item_mod(&mut self, i: &ItemMod) {
        if IncludeTests::No == self.include_tests && is_test_mod(i) {
            return;
        }
        visit::visit_item_mod(self, i);
    }

    fn visit_item_impl(&mut self, i: &ItemImpl) {
        // unsafe trait impl's
        self.metrics.counters.item_impls.count(i.unsafety.is_some());
        visit::visit_item_impl(self, i);
    }

    fn visit_item_trait(&mut self, i: &ItemTrait) {
        // Unsafe traits
        self.metrics
            .counters
            .item_traits
            .count(i.unsafety.is_some());
        visit::visit_item_trait(self, i);
    }

    fn visit_impl_item_method(&mut self, i: &ImplItemMethod) {
        if i.sig.unsafety.is_some() {
            self.enter_unsafe_scope()
        }
        self.metrics
            .counters
            .methods
            .count(i.sig.unsafety.is_some());
        visit::visit_impl_item_method(self, i);
        if i.sig.unsafety.is_some() {
            self.exit_unsafe_scope()
        }
    }

    // TODO: Visit macros.
    //
    // TODO: Figure out if there are other visit methods that should be
    // implemented here.
}
