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
use crate::bytecode::Instruction;
use super::{cw256,EvmStack,EvmState,ConcreteStack,ConcreteState,trace,UnknownMemory,UnknownStorage};

/// Identifies the dependency frames for each instruction in a given
/// sequence of instructions.
pub struct Dependencies {
    frames: Vec<Vec<Vec<usize>>>
}

impl Dependencies {
    fn new(width: usize) -> Self {
        // Initially empty set of frames to the required width.
        let frames = vec![Vec::new();width];
        Self{frames}
    }
    /// Determine the number of dependency frames for a given
    /// instruction.
    pub fn frames(&self, insn: usize) -> usize {
        self.frames[insn].len()
    }

    /// Get the _kth_ dependency frame for a given instruction.
    pub fn get_frame(&self, insn: usize, k: usize) -> &[usize] {
        &self.frames[insn][k]
    }
}

/// For a given bytecode sequence, identify the _dependency frames_
/// for all instructions.  A dependency frame contains an entry for
/// each operand of the instruction, where each entry identifies a
/// source instruction within the sequence.  If there are multiple
/// paths through the sequence to the given instruction, then there
/// may be multiple frames for a given instruction.
///
/// # Examples
/// The following illustrates a minimal example:
///
/// ```text
/// [0]   push 0x1
/// [1]   push 0x2
/// [2]   add          ;; [0,1]
/// ```
///
/// Here, instruction indices have been inserted to aid readability
/// and the dependency frames (if any) are given next to each
/// instruction.  In this case, there are no frames for the first two
/// instructions (i.e. as they have no operands).  For the `add`
/// instruction, we have one frame `[0,1]` which identifies the two
/// `push` instructions as its dependencies.
///
/// A more interesting example is:
///
/// ```text
/// [0]   calldatasize
/// [1]   push lab0
/// [2]   jumpi        ;; [1]
/// [3]   push 0x1
/// [4]   jump lab1    ;; [3]
/// lab0:
/// [5]   push 0x2
/// lab1:
/// [6]   neg          ;; [3][5]
/// ```
///
/// Here, we can see that the `neg` instruction has two dependency
/// frames indicating its dependency is either `push 0x1` _or_ `push
/// 0x2` (i.e. depending on which path was taken through the
/// control-flow graph).
pub fn find_dependencies(insns: &[Instruction], limit: usize) -> Result<Dependencies,Dependencies> {
    type Stack = DependencyStack<ConcreteStack<cw256>>;
    type Memory = UnknownMemory<cw256>;
    type Storage = UnknownStorage<cw256>;
    type State = ConcreteState<Stack,Memory,Storage>;
    // Construct initial state of EVM
    let init : State = State::new();
    // Run the abstract trace
    let mut err = false;
    let states : Vec<Vec<State>> = match trace(insns,init,limit) {
	Ok(states) => { states }
	Err(states) => { err = true; states }
    };
    // Convert over
    let map = build_insn_map(insns);
    let mut deps = Dependencies::new(states.len());
    //
    for (i,insn) in insns.iter().enumerate() {
        let nops = insn.operands(); // number of operands
        for state in &states[i] {
            // Extract dependencies            
            let st_deps = &match insn {
                // DUP is a special case
                Instruction::DUP(n) => {
                    let m = *n as usize;
                    state.stack().top_n(m-1,m)
                }
                // All the rest treated same
                _ => state.stack().top_n(0,nops)
            };
            // Convert byte offsets into instruction offsets
            let frame = st_deps.iter().map(|x| map[*x]).collect();
            // Push frame
            deps.frames[i].push(frame);
        }
        // Remove duplicates
        deps.frames[i].dedup();
    }
    //
    if err {
	Err(deps)
    } else {
	Ok(deps)
    }
}

fn build_insn_map(insns: &[Instruction]) -> Vec<usize> {
    let mut map = Vec::new();
    for (i,insn) in insns.iter().enumerate() {
        for _ in 0..insn.length() {
            map.push(i);
        }
    }
    map
}

/// A special "stack" which wraps around an existing stack, but
/// includes additional features.  In particular, it adds dependency
/// information as required to implement the functionality in this
/// file.
#[derive(Clone,Debug,Eq,Ord,PartialEq,PartialOrd)]
struct DependencyStack<T:EvmStack> {
    pc: usize,
    /// Inner stack implementing stack functionality however it
    /// wishes.
    stack: T,
    /// Mirrors `stack` with dependency information.  Thus, every
    /// entry matches an entry on `stack` and identifies the
    /// instruction where that entry was pushed on the stack.
    deps: Vec<usize>
}

impl<T:EvmStack> DependencyStack<T> {   
    /// Check that this dependency stack is consistent with the
    /// underlying stack.
    fn is_valid(&self) -> bool {
        self.stack.size() == self.deps.len()
    }
    
    /// Return the top `n..m` items from the stack.
    fn top_n(&self, n: usize, m:usize) -> &[usize] {
        let n0 = self.deps.len() - n;
        let m0 = self.deps.len() - m;
        &self.deps[m0..n0]
    }
}

impl<T:EvmStack> EvmStack for DependencyStack<T> {
    type Word = T::Word;
    
    fn size(&self) -> usize {
        assert!(self.is_valid()); 
        self.stack.size()
    }

    fn peek(&self, n: usize) -> &Self::Word {
        assert!(self.is_valid());         
        self.stack.peek(n)
    }

    fn push(&mut self, item: Self::Word) {
        assert!(self.is_valid());         
        self.stack.push(item);
        self.deps.push(self.pc);
    }

    fn pop(&mut self) -> Self::Word {
        assert!(self.is_valid());
        self.deps.pop().unwrap();
        self.stack.pop()
    }

    fn set(&mut self, n: usize, item: Self::Word) -> Self::Word {
        assert!(self.is_valid());        
        let i = self.deps.len() - (n+1);
        self.deps[i] = self.pc;
        self.stack.set(n,item)
    }

    fn swap(&mut self, n: usize) {
        let i = self.deps.len() - (n+1);
        let j = self.deps.len() - 1;
        self.stack.swap(n);
        self.deps.swap(i,j);
    }        

    fn goto(&mut self, pc: usize) {
        self.stack.goto(pc);
        self.pc = pc;
    }
}

impl<T:EvmStack+Default> Default for DependencyStack<T> {
    fn default() -> Self {
        let stack = T::default();
        let deps = Vec::new();
        let me = Self{pc:0,stack,deps};
        assert!(me.is_valid());
        me
    }                         
}
