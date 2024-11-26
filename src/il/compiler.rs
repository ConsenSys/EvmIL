// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
use crate::il::{BinOp, Region, Term};
use crate::bytecode::{Assembly,Builder,Instruction,StructuredSection};
use crate::bytecode::Instruction::*;
use crate::util::*;

type Result = std::result::Result<(), CompilerError>;

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug)]
pub enum CompilerError {
    /// An integer (or hex) literal is too large (i.e. exceeds `2^256`).
    LiteralOverflow,
    /// Attempt to read from an invalid memory region.
    InvalidMemoryAccess,
    /// Attempt to write something which doesn't exist, or is not an lval.
    InvalidLVal,
}

// ============================================================================
// Compiler
// ============================================================================

pub struct Compiler {
    /// Instructions being constructed by this compiler.
    builder: Builder,
    /// Counts the number of labels in use
    labels: usize
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            builder: Builder::new(),
            labels: 0
        }
    }

    pub fn to_assembly(self) -> Assembly {
        let insns = self.builder.to_insns();
        let code = StructuredSection::Code(insns);
        Assembly::new(vec![code])        
    }

    pub fn translate(&mut self, term: &Term) -> Result {
        match term {
            // Statements
            Term::Assert(e) => self.translate_assert(e),
            Term::Assignment(e1, e2) => self.translate_assignment(e1, e2),
            Term::Fail => self.translate_fail(),
            Term::Goto(l) => self.translate_goto(l),
            Term::IfGoto(e, l) => self.translate_ifgoto(e, l),
            Term::Label(l) => self.translate_label(l),
            Term::Return(es) => self.translate_return(es),
            Term::Revert(es) => self.translate_revert(es),
            Term::Succeed(es) => self.translate_succeed(es),
            Term::Stop => self.translate_stop(),
            // Expressions
            Term::Binary(bop, e1, e2) => self.translate_binary(*bop, e1, e2),
            Term::Call(n,es) => self.translate_call(n,es),
            Term::ArrayAccess(src, index) => self.translate_array_access(src, index),
            Term::MemoryAccess(_) => Err(CompilerError::InvalidMemoryAccess),
            // Values
            Term::Int(bytes) => self.translate_literal(bytes, 10),
            Term::Hex(bytes) => self.translate_literal(bytes, 16),
            //
        }
    }

    fn fresh_label(&mut self) -> String {
        let lab = format!("lab{}",self.labels);
        self.labels += 1;
        lab
    }

    // ============================================================================
    // Statements
    // ============================================================================

    fn translate_assert(&mut self, expr: &Term) -> Result {
        // Allocate labels for true/false outcomes
        let lab = self.fresh_label();
        // Translate conditional branch
        self.translate_conditional(expr, Some(&lab), None)?;
        // False branch
        self.builder.push(PUSH(vec![0x00]));
        self.builder.push(PUSH(vec![0x00]));        
        self.builder.push(REVERT);
        // True branch
        self.builder.mark_label(&lab).unwrap();
        self.builder.push(JUMPDEST);
        //
        Ok(())
    }

    fn translate_assignment(&mut self, lhs: &Term, rhs: &Term) -> Result {
        // Translate value being assigned
        self.translate(rhs)?;
        // Translate assignent itself
        match lhs {
            Term::ArrayAccess(src, idx) => {
                self.translate_assignment_array(src, idx)?;
            }
            _ => {
                return Err(CompilerError::InvalidLVal);
            }
        }
        //
        Ok(())
    }

    fn translate_assignment_array(&mut self, src: &Term, index: &Term) -> Result {
        match src {
            Term::MemoryAccess(r) => self.translate_assignment_memory(*r, index),
            _ => Err(CompilerError::InvalidMemoryAccess),
        }
    }

    fn translate_assignment_memory(&mut self, region: Region, address: &Term) -> Result {
        // Translate index expression
        self.translate(address)?;
        // Dispatch based on region
        match region {
            Region::Memory => self.builder.push(MSTORE),
            Region::Storage => self.builder.push(SSTORE),
            _ => {
                return Err(CompilerError::InvalidMemoryAccess);
            }
        };
        //
        Ok(())
    }

    fn translate_call(&mut self, name: &str, exprs: &[Term]) -> Result {
        let retlab = self.fresh_label();
        let retlab_index = self.builder.get_label(&retlab);
        let name_index = self.builder.get_label(name);
        // Translate arguments
        for e in exprs { self.translate(e)?; }
        // Push return address (as a label)
        self.builder.push_labeled(PUSH(label_bytes(retlab_index)));
        // Push function address (as a label)
        self.builder.push_labeled(PUSH(label_bytes(name_index)));
        // Perform jump
        self.builder.push(JUMP);
        // Identify return point
        self.builder.mark_label(&retlab).unwrap();
        self.builder.push(JUMPDEST);
        Ok(())
    }

    fn translate_fail(&mut self) -> Result {
        self.builder.push(PUSH(vec![0x00]));
        self.builder.push(PUSH(vec![0x00]));        
        self.builder.push(REVERT);
        Ok(())
    }

    fn translate_goto(&mut self, label: &str) -> Result {
        let label_index = self.builder.get_label(label);        
        // Translate unconditional branch
        self.builder.push_labeled(PUSH(label_bytes(label_index)));
        self.builder.push(JUMP);
        //
        Ok(())
    }

    fn translate_ifgoto(&mut self, expr: &Term, label: &str) -> Result {
        // Translate conditional branch
        self.translate_conditional(expr, Some(label), None)
    }

    fn translate_label(&mut self, label: &str) -> Result {
        // Mark the label
        self.builder.mark_label(label).unwrap();
        self.builder.push(JUMPDEST);
        // Done
        Ok(())
    }

    fn translate_return(&mut self, exprs: &[Term]) -> Result {
        if !exprs.is_empty() {
            // Translate each expression (except first)
            for e in exprs.iter().skip(1) { self.translate(e)?; }
            // Translate first expression
            self.translate(&exprs[0])?;
            // Swap with returna address
            self.builder.push(SWAP(exprs.len() as u8));
        }
        // A return statement is just an unconditional jump
        self.builder.push(JUMP);
        //
        Ok(())
    }

    fn translate_revert(&mut self, exprs: &[Term]) -> Result {
        self.translate_succeed_revert(REVERT, exprs)
    }

    fn translate_succeed(&mut self, exprs: &[Term]) -> Result {
        if exprs.is_empty() {
            self.builder.push(STOP);
            Ok(())
        } else {
            self.translate_succeed_revert(RETURN, exprs)
        }
    }

    fn translate_succeed_revert(&mut self, insn: Instruction, exprs: &[Term]) -> Result {
        if exprs.is_empty() {
            self.builder.push(PUSH(vec![0]));
            self.builder.push(PUSH(vec![0]));
        } else {
            for (i,e) in exprs.iter().enumerate() {
                let addr = (i * 0x20) as u128;
                self.translate(e)?;
                self.builder.push(make_push(addr)?);
                self.builder.push(MSTORE);
            }
            let len = (exprs.len() * 0x20) as u128;
            self.builder.push(PUSH(vec![0]));
            self.builder.push(make_push(len)?);
        }
        self.builder.push(insn);
        Ok(())
    }

    fn translate_stop(&mut self) -> Result {
        self.builder.push(STOP);
        Ok(())
    }

    // ============================================================================
    // Conditional Expressions
    // ============================================================================

    /// Translate a conditional expression using _short circuiting_
    /// semantics.  Since implementing short circuiting requires
    /// branching, we can exploit this to optimise other translations
    /// and reduce the number of branches required.  To do that, this
    /// method requires either a `true` target or a `false` target.
    /// If a `true` target is provided then, if the condition
    /// evaluates to something other than `0` (i.e. is `true`),
    /// control is transferred to this target.  Likewise, if the
    /// condition evaluates to `0` (i.e. `false`) the control is
    /// transferred to the `false` target.
    fn translate_conditional(&mut self, expr: &Term, true_lab: Option<&str>, false_lab: Option<&str>) -> Result {
        match expr {
            Term::Binary(BinOp::LogicalAnd, l, r) => {
                self.translate_conditional_conjunct(l, r, true_lab, false_lab)
            }
            Term::Binary(BinOp::LogicalOr, l, r) => {
                self.translate_conditional_disjunct(l, r, true_lab, false_lab)
            }
            _ => self.translate_conditional_other(expr, true_lab, false_lab),
        }
    }

    /// Translate a logical conjunction as a conditional. Since
    /// such connectives require short circuiting, these must be
    /// implementing using branches.
    fn translate_conditional_conjunct(&mut self, lhs: &Term, rhs: &Term, true_lab: Option<&str>, false_lab: Option<&str>) -> Result {
        match (true_lab, false_lab) {
            (Some(_), None) => {
                // Harder case
                let lab = self.fresh_label();
                self.translate_conditional(lhs, None, Some(&lab))?;
                self.translate_conditional(rhs, true_lab, None)?;
                self.builder.mark_label(&lab).unwrap();
                self.builder.push(JUMPDEST);
            }
            (None, Some(_)) => {
                // Easy case
                self.translate_conditional(lhs, None, false_lab)?;
                self.translate_conditional(rhs, true_lab, false_lab)?;
            }
            (_, _) => unreachable!(),
        }
        // Done
        Ok(())
    }

    /// Translate a logical disjunction as a conditional. Since
    /// such connectives require short circuiting, these must be
    /// implementing using branches.
    fn translate_conditional_disjunct(&mut self, lhs: &Term, rhs: &Term, true_lab: Option<&str>, false_lab: Option<&str>) -> Result {
        match (true_lab, false_lab) {
            (None, Some(_)) => {
                // Harder case
                let lab = self.fresh_label();
                self.translate_conditional(lhs, Some(&lab), None)?;
                self.translate_conditional(rhs, None, false_lab)?;
                self.builder.mark_label(&lab).unwrap();
                self.builder.push(JUMPDEST);
            }
            (Some(_), None) => {
                // Easy case
                self.translate_conditional(lhs, true_lab, None)?;
                self.translate_conditional(rhs, true_lab, false_lab)?;
            }
            (_, _) => unreachable!(),
        }
        // Done
        Ok(())
    }

    /// Translate a conditional expression which cannot be translated
    /// by exploiting branches.  In such case, we have to generate the
    /// boolean value and dispatch based on that.
    fn translate_conditional_other(&mut self, expr: &Term, true_lab: Option<&str>, false_lab: Option<&str>) -> Result {
        // Translate conditional expression
        self.translate(expr)?;
        //
        match (true_lab, false_lab) {
            (Some(lab), None) => {
                let label_index = self.builder.get_label(lab);
                self.builder.push_labeled(PUSH(label_bytes(label_index)));
                self.builder.push(JUMPI);
            }
            (None, Some(lab)) => {
                let label_index = self.builder.get_label(lab);
                self.builder.push(ISZERO);
                self.builder.push_labeled(PUSH(label_bytes(label_index)));
                self.builder.push(JUMPI);
            }
            (_, _) => {
                unreachable!("")
            }
        }
        //
        Ok(())
    }

    // ============================================================================
    // Binary Expressions
    // ============================================================================

    /// Translate a binary operation.  Observe that logical operations
    /// exhibit _short-circuit behaviour_.
    fn translate_binary(&mut self, bop: BinOp, lhs: &Term, rhs: &Term) -> Result {
        match bop {
            BinOp::LogicalAnd | BinOp::LogicalOr => {
                self.translate_logical_connective(bop, lhs, rhs)
            }
            _ => self.translate_binary_arithmetic(bop, lhs, rhs),
        }
    }

    /// Translate one of the logical connectives (e.g. `&&` or `||`).
    /// These are more challenging than standard binary operators because
    /// they exhibit _short circuiting behaviour_.
    fn translate_logical_connective(&mut self, bop: BinOp, lhs: &Term, rhs: &Term) -> Result {
        self.translate(lhs)?;
        self.builder.push(DUP(1));
        if bop == BinOp::LogicalAnd {
            self.builder.push(ISZERO);
        }
        // Allocate fresh label
        let lab = self.fresh_label();
        let lab_index = self.builder.get_label(&lab);
        self.builder.push_labeled(PUSH(label_bytes(lab_index)));
        self.builder.push(JUMPI);
        self.builder.push(POP);
        self.translate(rhs)?;
        self.builder.mark_label(&lab).unwrap();
        self.builder.push(JUMPDEST);
        // Done
        Ok(())
    }

    /// Translate a binary arithmetic operation or comparison.  This is
    /// pretty straightforward, as we just load items on the stack and
    /// perform the op.  Observe that the right-hand side is loaded onto
    /// the stack first.
    fn translate_binary_arithmetic(&mut self, bop: BinOp, lhs: &Term, rhs: &Term) -> Result {
        self.translate(rhs)?;
        self.translate(lhs)?;
        //
        match bop {
            // standard
            BinOp::Add => self.builder.push(ADD),
            BinOp::Subtract => self.builder.push(SUB),
            BinOp::Divide => self.builder.push(DIV),
            BinOp::Multiply => self.builder.push(MUL),
            BinOp::Remainder => self.builder.push(MOD),
            BinOp::Equals => self.builder.push(EQ),
            BinOp::LessThan => self.builder.push(LT),
            BinOp::GreaterThan => self.builder.push(GT),
            // non-standard
            BinOp::NotEquals => {
                self.builder.push(EQ);
                self.builder.push(ISZERO);
            }
            BinOp::LessThanOrEquals => {
                self.builder.push(GT);
                self.builder.push(ISZERO);
            }
            BinOp::GreaterThanOrEquals => {
                self.builder.push(LT);
                self.builder.push(ISZERO);
            }
            _ => {
                unreachable!();
            }
        }
        //
        Ok(())
    }

    // ============================================================================
    // Array Access Expressions
    // ============================================================================

    /// Translate an array access of the form `src[index]`.  The actual
    /// form of the translation depends on whether its a direct access
    /// (e.g. to storage or memory), or indirect (e.g. via a pointer to
    /// memory).
    fn translate_array_access(&mut self, src: &Term, index: &Term) -> Result {
        match src {
            Term::MemoryAccess(r) => self.translate_memory_access(*r, index),
            _ => Err(CompilerError::InvalidMemoryAccess),
        }
    }

    fn translate_memory_access(&mut self, region: Region, index: &Term) -> Result {
        // Translate index expression
        self.translate(index)?;
        // Dispatch based on region
        match region {
            Region::Memory => {
                self.builder.push(MLOAD);
            }
            Region::Storage => {
                self.builder.push(SLOAD);
            }
            Region::CallData => {
                self.builder.push(CALLDATALOAD);
            }
        }
        //
        Ok(())
    }

    // ============================================================================
    // Values
    // ============================================================================

    fn translate_literal(&mut self, digits: &[u8], radix: u32) -> Result {
        let val = from_be_digits(digits, radix);
        self.builder.push(make_push(val)?);
        Ok(())
    }
}

// ===================================================================
// Traits
// ===================================================================

/// Translate a sequence of IL statements into EVM bytecode, or fail
/// with an error.
impl TryFrom<&[Term]> for Assembly {
    type Error = CompilerError;

    fn try_from(terms: &[Term]) -> std::result::Result<Self, Self::Error> {
        try_from(terms)
    }
}

/// Translate a sequence of IL statements into EVM bytecode, or fail
/// with an error.
impl<const N: usize> TryFrom<&[Term; N]> for Assembly {
    type Error = crate::il::CompilerError;

    fn try_from(terms: &[Term; N]) -> std::result::Result<Self, Self::Error> {
        try_from(terms)
    }
}

// ===================================================================
// Helpers
// ===================================================================

fn label_bytes(index: usize) -> Vec<u8> {
    // Always generate a push2 instruction
    vec![(index / 256) as u8, (index % 256) as u8]
}

fn try_from(terms: &[Term]) -> std::result::Result<Assembly, CompilerError> {
    let mut compiler = Compiler::new();
    // Translate statements one-by-one
    for t in terms {
        compiler.translate(t)?;
    }
    // Done
    Ok(compiler.to_assembly())
}

/// Construct a push instruction from a value.
fn make_push(val: u128) -> std::result::Result<Instruction, CompilerError> {
    let bytes = to_be_bytes(val);
    // Sanity check size of literal
    if bytes.len() > 32 {
        // Too big!!
        Err(CompilerError::LiteralOverflow)
    } else {
        Ok(PUSH(bytes))
    }
}
