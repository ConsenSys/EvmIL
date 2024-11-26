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
use std::slice::{Iter,IterMut};
use super::{Instruction};
use super::{eof,legacy};
pub use super::eof::DecodingError;
use super::ParseError;

// ============================================================================
// Structured Contract
// ============================================================================

/// A structured representation of an EVM bytecode contract which is
/// either a _legacy contract_, or an EVM Object Format (EOF)
/// compatible contract.  Regardless of whether it is legacy or not,
/// a contract is divided into one or more _sections_.  A section is
/// either a _code section_ or a _data section_.  For EOF contracts,
/// the _data section_ should also come last.  However, for legacy
/// contracts, they can be interleaved.
#[derive(Clone,Debug,PartialEq)]
pub struct Assembly {
    sections: Vec<StructuredSection>
}

impl Assembly {

    pub fn from_legacy_bytes(bytes: &[u8]) -> Assembly {
        legacy::from_bytes(bytes)
    }

    /// A decoded EOF byte sequence (see
    /// [EIP3540](https://eips.ethereum.org/EIPS/eip-3540)).  This
    /// provides a gateway for disassembling EOF contracts into assembly
    /// language and back again.
    ///
    /// # Examples
    /// ```
    /// use evmil::bytecode::Assembly;
    /// use evmil::util::FromHexString;
    ///
    /// // EOF bytecode contract
    /// let hex = "0xef00010100040200010001030000000000000000";
    /// // Conversion into bytes
    /// let bytes = hex.from_hex_string().unwrap();
    /// // Decode EOF bytecode (assuming no errors)
    /// let eof = Assembly::from_eof_bytes(&bytes).unwrap();
    /// // Check that section contains one instruction
    /// // assert_eq!(eof.sections.len(),1);
    /// ```    
    pub fn from_eof_bytes(bytes: &[u8]) -> Result<Assembly,DecodingError> {
        eof::from_bytes(bytes)
    }
    
    pub fn empty() -> Self {
        Self {
            sections: Vec::new()
        }
    }

    pub fn new(sections: Vec<StructuredSection>) -> Self {
        Self { sections }
    }

    /// Return the number of sections in the code.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Return `true` if the are no sections in this assembly.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    pub fn iter(&self) -> Iter<'_,StructuredSection> {
        self.sections.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<'_,StructuredSection> {
        self.sections.iter_mut()
    }

    /// Add a new section to this bytecode container
    pub fn add(&mut self, section: StructuredSection) {
        self.sections.push(section)
    }

    /// Parse some assembly language into an `Assembly`.  This can
    /// fail for a variety of reasons, such as an unknown instruction
    /// is used or there is some unexpected junk in the file.
    pub fn from_str(input: &str) -> Result<Assembly,ParseError> {
        let parser = super::parser::Parser::new(input);
        parser.parse()
    }    
}

impl Assembly {
    pub fn to_legacy_bytes(&self) -> Vec<u8> {
        legacy::to_bytes(self)
    }

    pub fn to_eof_bytes(&self) -> Vec<u8> {
        eof::to_bytes(self).unwrap()
    }    
}    

// ===================================================================
// Traits
// ===================================================================

impl<'a> IntoIterator for &'a Assembly {
    type Item = &'a StructuredSection;
    type IntoIter = Iter<'a,StructuredSection>;

    fn into_iter(self) -> Self::IntoIter {
        self.sections.iter()
    }
}

impl<'a> IntoIterator for &'a mut Assembly {
    type Item = &'a mut StructuredSection;
    type IntoIter = IterMut<'a,StructuredSection>;

    fn into_iter(self) -> Self::IntoIter {
        self.sections.iter_mut()
    }
}

// ============================================================================
// Section
// ============================================================================

#[derive(Clone,Debug,PartialEq)]
pub enum StructuredSection {
    /// A data section is simply a sequence of zero or more bytes.
    Data(Vec<u8>),
    /// A code section is a sequence of zero or more instructions
    /// along with appropriate _metadata_.
    Code(Vec<Instruction>)
}
