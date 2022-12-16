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
use std::{cmp,fmt};
use crate::{Instruction,Instruction::*};
use crate::{AbstractState};
use crate::util;

const MAX_CODE_SIZE : u128 = 24576;

/// Bottom represents an _unvisited_ state.
const BOTTOM : CfaState = CfaState{stack: None};

// ============================================================================
// Abstract Value
// ============================================================================

/// An abstract value is either a known constant, or an unknown
/// (i.e. arbitrary value).
#[derive(Clone,Copy,Debug,PartialEq)]
pub enum Value {
    Known(usize),
    Unknown
}

impl Value {
    pub fn merge(self, other: Value) -> Value {
        if self == other {
            self
        } else {
            Value::Unknown
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Unknown => write!(f,"(??)"),
            Value::Known(n) => write!(f,"({:#08x})",n)
        }
    }
}
// ============================================================================
// Disassembly Context
// ============================================================================

#[derive(Debug,PartialEq)]
pub struct CfaState {
    stack: Option<Vec<Value>>
}

impl CfaState {
    pub fn is_bottom(&self) -> bool {
        self.stack.is_none()
    }
    /// Pop an item of this stack, producing an updated state.
    pub fn len(&self) -> usize {
        match self.stack {
            Some(ref stack) => stack.len(),
            None => 0
        }
    }
    /// Push an iterm onto this stack.
    pub fn push(self, val: Value) -> Self {
        let st = match self.stack {
            Some(mut stack) => {
                // Pop target address off the stack.
                stack.push(val);
                stack
            }
            None => {
                let mut stack = Vec::new();
                stack.push(val);
                stack
            }
        };
        CfaState{stack:Some(st)}
    }
    /// Pop an item of this stack, producing an updated state.
    pub fn pop(self) -> Self {
        match self.stack {
            Some(mut stack) => {
                // Pop target address off the stack.
                stack.pop();
                // Done
                CfaState{stack:Some(stack)}
            }
            None => {
                panic!("stack underflow");
            }
        }
    }
    /// Perk nth item on the stack (where `0` is top).
    pub fn peek(&self, n: usize) -> Value {
        match self.stack {
            Some(ref stack) => {
                stack[stack.len() - (1+n)]
            }
            None => {
                panic!("stack underflow");
            }
        }
    }
    /// Set specific item on this stack.
    pub fn set(self, n: usize, val: Value) -> Self {
        let mut st = match self.stack {
            Some(mut stack) => {
                stack
            }
            None => {
                panic!("stack underflow");
            }
        };
        let m = st.len() - (1+n);
        // Make the assignment
        st[m] = val;
        // Done
        CfaState{stack:Some(st)}
    }
}

impl Default for CfaState {
    fn default() -> Self { BOTTOM }
}

impl Clone for CfaState {
    fn clone(&self) -> Self {
        CfaState{stack:self.stack.clone()}
    }
}

impl fmt::Display for CfaState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.stack {
            None => write!(f,"_|_"),
            Some(ref stack) => {
                write!(f,"[")?;
                for i in 0..stack.len() {
                    write!(f,"{}",stack[i])?;
                }
                write!(f,"]")
            }
        }
    }
}

impl AbstractState for CfaState {
    fn is_reachable(&self) -> bool { self.stack.is_some() }

    fn branch(&self, pc: usize, insn: &Instruction) -> Self {
        match insn {
            JUMPI => self.clone().pop().pop(),
            JUMP => self.clone().pop(),
            _ => {
                unreachable!()
            }
        }
    }

    fn merge(&mut self, other: Self) -> bool {
        if self.is_bottom() {
            if !other.is_bottom() {
                *self = other;
                return true;
            }
        } else if !other.is_bottom() {
            if self.stack != other.stack {
                let s_len = self.stack.as_ref().unwrap().len();
                let o_len = other.stack.as_ref().unwrap().len();
                // Determine height of new stack
                let m = cmp::min(s_len,o_len);
                // Construct a new stack
                let mut nstack = Vec::new();
                // Perform stack merge
                for i in (0..m).rev() {
                    let l = self.peek(i);
                    let r = other.peek(i);
                    nstack.push(l.merge(r));
                }
                // Update me
                *self = CfaState{stack:Some(nstack)}
            }
        }
        //
        false
    }

    fn top(&self) -> usize {
        // Extract the stack.  We assume for now we are not bottom.
        let stack = self.stack.as_ref().unwrap();
        // Inspect last element.  Again, we assume for now this
        // exists.
        match stack.last().unwrap() {
            Value::Known(n) => *n,
            Value::Unknown => {
                // At some point, this will need to be fixed.
                panic!("Unknown value encountered");
            }
        }
    }

    // ============================================================================
    // Abstract Instruction Semantics (stack)
    // ============================================================================

    /// Update an abstract state with the effects of a given instruction.
    fn transfer(self, insn: &Instruction) -> CfaState {
        match insn {
            STOP => BOTTOM,
            // 0s: Stop and Arithmetic Operations
            ADD|MUL|SUB|DIV|SDIV|MOD|SMOD|EXP|SIGNEXTEND => {
                self.pop().pop().push(Value::Unknown)
            }
            ADDMOD|MULMOD => {
                self.pop().pop().pop().push(Value::Unknown)
            }
            // 0s: Stop and Arithmetic Operations
            ISZERO|NOT => {
                self.pop().push(Value::Unknown)
            }
            // Binary Comparators
            LT|GT|SLT|SGT|EQ => {
                self.pop().pop().push(Value::Unknown)
            }
            // Binary bitwise operators
            AND|OR|XOR|BYTE|SHL|SHR|SAR => {
                self.pop().pop().push(Value::Unknown)
            }
            // 20s: Keccak256
            // 30s: Environmental Information
            CALLVALUE => self.push(Value::Unknown),
            CALLDATALOAD => self.pop().push(Value::Unknown),
            CALLDATASIZE => self.push(Value::Unknown),
            // 40s: Block Information
            // 50s: Stack, Memory, Storage and Flow Operations
            POP => self.pop(),
            MLOAD => self.pop().push(Value::Unknown),
            MSTORE => self.pop().pop(),
            SLOAD => self.pop().push(Value::Unknown),
            SSTORE => self.pop().pop(),
            JUMPI => self.pop().pop(),
            JUMPDEST(_) => self, // nop
            // 60 & 70s: Push Operations
            PUSH(bytes) => {
                let n = util::from_be_bytes(&bytes);
                if n <= MAX_CODE_SIZE {
                    self.push(Value::Known(n as usize))
                } else {
                    self.push(Value::Unknown)
                }
            }
            // 80s: Duplicate Operations
            DUP(n) => {
                let m = (*n - 1) as usize;
                let nth = self.peek(m);
                self.push(nth)
            }
            // 90s: Swap Operations
            SWAP(n) => {
                let m = (*n - 1) as usize;
                let x = self.peek(m);
                let y = self.peek(0);
                self.set(0,x).set(m,y)
            }
            // 90s: Exchange Operations
            // a0s: Logging Operations
            // f0s: System Operations
            INVALID|JUMP|RETURN|REVERT|STOP => {
                BOTTOM
            }
            _ => {
                // This is a catch all to ensure no instructions are
                // missed above.
                panic!("unknown instruction ({:?})",insn);
            }
        }
    }
}