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
use super::{cw256,ConcreteStack,ConcreteState,trace,UnknownMemory,UnknownStorage};

/// For a given bytecode sequence, identify all _reachable_
/// instructions.  An instruction is reachable if there exists a path
/// from the first instruction in the sequence to the given
/// instruction.  For example, consider this sequence:
///
/// ```txt
///    push lab
///    jumpi
///    stop
///    pop
/// lab:
///    stop
/// ```
///
/// The reachability analysis would conclude that the `pop`
/// instruction here is _unreachable_.  That is because there is no
/// path through the control-flow graph which can lead to it.
pub fn find_reachable(insns: &[Instruction], limit: usize) -> Result<Vec<bool>,()> {
    // Configure analysis
    type Stack = ConcreteStack<cw256>;
    type Memory = UnknownMemory<cw256>;
    type Storage = UnknownStorage<cw256>;
    type State = ConcreteState<Stack,Memory,Storage>;    
    // Construct initial state of EVM
    let init = State::new();
    // Run the abstract trace
    let states : Vec<Vec<State>> = trace(insns,init,limit).map_err(|e| ())?;
    // Convert output into boolean reachability info
    let mut flags = Vec::new();
    //
    for st in states {
        // Check whether corresponding instruction was reached during
        // the trace.
        let reached = !st.is_empty();
        //
        flags.push(reached);
    }
    // Done
    Ok(flags)
}
