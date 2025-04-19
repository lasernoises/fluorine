#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use std::{borrow::Cow, cell::RefCell, rc::Rc};

use eframe::egui;
use fluorine::*;
use parser::{Expr, Parser, parse, tokenize_with_context};

mod parser;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        ..Default::default()
    };
    eframe::run_native(
        "Fluorine Spreadsheet",
        options,
        Box::new(|_| Ok(Box::new(Spreadsheet::default()))),
    )
}

type Cells = [(Rc<str>, Rx<Option<Expr>>, RefCell<RxFn<(), Option<f64>>>); 4];

struct Spreadsheet {
    dependent: Rc<Dependent>,
    cells: Cells,
}

impl Spreadsheet {
    fn eval_cell(&self, ctx: &mut RxCtx, i: usize) -> Option<f64> {
        let cell = &self.cells.get(i)?;

        let Ok(mut rx_fn) = cell.2.try_borrow_mut() else {
            // If can't get a lock on the RxFn because we are being evaluated by it due to a cycle
            // we track its Expr as a fallback so that reactivity doesn't get lost. Otherwise if the
            // cell changes in a way that would break the cycle we wouldn't get invalidated.
            cell.1.get(ctx);

            return None;
        };

        *rx_fn.call(ctx, (), |ctx, _| {
            eval(cell.1.get(ctx).as_ref()?, &mut |i| self.eval_cell(ctx, i))
        })
    }
}

impl Default for Spreadsheet {
    fn default() -> Self {
        Self {
            dependent: Dependent::toplevel(),
            cells: std::array::from_fn(|_| {
                (Rc::from(""), Rx::new(None), RefCell::new(RxFn::new()))
            }),
        }
    }
}

impl eframe::App for Spreadsheet {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for i in 0..4 {
                        ui.horizontal(|ui| {
                            ui.label(format!("${} =", i));

                            let mut tmp: Cow<str> = Cow::Borrowed(&self.cells[i].0);
                            ui.text_edit_singleline(&mut tmp);

                            if let Cow::Owned(new) = tmp {
                                // TODO: improve performance / reduce allocations

                                let tokens = tokenize_with_context(&new);

                                let mut parser = Parser::new(&tokens);

                                let expr = parse(&mut parser);

                                self.cells[i].0 = Rc::from(new);
                                *self.cells[i].1.get_mut() = dbg!(expr.ok());
                            }

                            ui.label("=");
                            ui.label(
                                self.eval_cell(&mut self.dependent.ctx(), i)
                                    .map(|r| r.to_string())
                                    .as_deref()
                                    .unwrap_or("error"),
                            );
                        });
                    }
                });
            });
        });
    }
}

fn eval(expr: &Expr, eval_other: &mut impl FnMut(usize) -> Option<f64>) -> Option<f64> {
    match expr {
        Expr::Binary(left, operator, right) => {
            let left = eval(left, eval_other)?;
            let right = eval(right, eval_other)?;

            Some(match operator {
                parser::BinaryOperator::Slash => left / right,
                parser::BinaryOperator::Star => left * right,
                parser::BinaryOperator::Plus => left + right,
                parser::BinaryOperator::Minus => left - right,
            })
        }
        Expr::Grouping(expr) => eval(expr, eval_other),
        Expr::Number(num) => Some(*num),
        Expr::Unary(operator, expr) => {
            let val = eval(expr, eval_other)?;

            Some(match operator {
                parser::UnaryOperator::Minus => -val,
            })
        }
        Expr::Variable(ident) => {
            let i: usize = ident.parse().ok()?;

            eval_other(i)
        }
    }
}
