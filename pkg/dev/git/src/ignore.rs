use base_error::*;
use file::{GlobFileFilter, LocalPath};
use automata::regexp::vm::instance::RegExp;

/*
Glob notes
- See https://git-scm.com/docs/gitignore
- In gitignore
    - 'hello.*' match in any directory.
    - '/hello.*' : only match in current directory

    - '#' at the start of a line is a comment
        - must escape as '\#'
        - similarly must escape trailing whitespace

    - '!' in front of a line excludes the pattern
        -

- The '/' at the beginning behavior is unique to git. For other systems, there is a similar concept of matching in any directory by default.


*/

pub struct GitIgnore {
    entries: Vec<Entry>
}

struct Entry {
    only_select_directories: bool,
    pattern: RegExp
}

impl GitIgnore {

    // TODO: Implement all features in this.
    pub async fn read() -> Result<Self> {
        let mut entries = vec![];

        let base_dir = file::project_dir();

        // TODO: Skip if this file is missing.
        let mut data = file::read_to_string(file::project_path!(".gitignore")).await?;

        // Implicit ignored directory.
        data.push_str("\n/.git\n");

        for line in data.lines() {
            let mut line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Comment
            if line.starts_with("#") {
                continue;
            }

            let mut only_select_directories = false;
            while let Some(v) = line.strip_suffix("/") {
                line = v;
                only_select_directories = true;
            }

            let is_relative_current_dir = line.contains('/');

            while let Some(v) = line.strip_prefix("/") {
                line = v;
            }

            let mut line = line.to_string();
            if !is_relative_current_dir {
                line = format!("**/{}", line);
            }

            let path = base_dir.join(line).normalized();

            let pattern = GlobFileFilter::compile_glob(&path)?;

            entries.push(Entry {
                only_select_directories, pattern
            });
        }

        Ok(Self {
            entries
        })
    }

    pub fn should_ignore(&self, path: &LocalPath, is_directory: bool) -> bool {
        for entry in &self.entries {

            if !entry.pattern.test(path.as_str()) {
                continue;
            }

            if entry.only_select_directories && !is_directory {
                continue;
            }

            return true;
        }
        
        false
    }
}
