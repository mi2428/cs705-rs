//! Native option producer, distinct from an explicit numeric protocol mask.

use super::{Error, invalid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneOptionButton {
    Ok,
    Cancel,
    FormEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOptions {
    pub word: u32,
    pub states: [u8; 5],
}
impl CloneOptions {
    /// Clear bit 0 BEFORE constructing controls. Each later control reads live
    /// word independently; a failed producer retains all preceding changes.
    pub fn open(word: u32) -> Self {
        Self {
            word: word & !1,
            states: [0; 5],
        }
    }
    pub fn initialize(&mut self, index: usize) -> Result<(), Error> {
        let state = self
            .states
            .get_mut(index)
            .ok_or(invalid("clone checkbox index outside0..4"))?;
        *state = ((self.word >> index) & 1) as u8;
        Ok(())
    }
    /// Save each low AL/store before reading the next widget; high bits survive.
    pub fn save(
        &mut self,
        button: CloneOptionButton,
        mut getter: impl FnMut(usize, &mut Self) -> Result<u32, Error>,
    ) -> Result<bool, Error> {
        for index in 0..5 {
            let al = getter(index, self)? as u8;
            let bit = 1 << index;
            self.word = if al == 0 {
                self.word & !bit
            } else {
                self.word | bit
            };
        }
        Ok(button == CloneOptionButton::Ok)
    }
    /// TCheckBox Checked is state==1; gray state 2 is false.
    pub fn press(&mut self, button: CloneOptionButton) -> Result<bool, Error> {
        self.save(button, |index, state| {
            Ok(u32::from(state.states[index] == 1))
        })
    }
    pub fn mask(&self) -> u8 {
        self.word as u8 & 31
    }
}
