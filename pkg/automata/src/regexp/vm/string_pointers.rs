use crate::regexp::vm::instruction::StringPointer;

#[derive(Clone, Default)]
pub struct SavedStringPointers {
    // List of pointers. StringPointer::MAX is used as a 'None' indicator to save memory.
    list: Vec<StringPointer>,
}

impl SavedStringPointers {
    pub fn get(&self, index: usize) -> Option<StringPointer> {
        self.list.get(index).and_then(|v| {
            if *v == StringPointer::MAX {
                None
            } else {
                Some(*v)
            }
        })
    }

    pub fn set(&mut self, index: usize, value: StringPointer) {
        if self.list.len() <= index {
            self.list.resize(index + 1, StringPointer::MAX);
        }

        self.list[index] = value;
    }

    pub fn to_vec(&self) -> Vec<Option<StringPointer>> {
        (0..self.list.len()).map(|i| self.get(i)).collect()
    }
}
