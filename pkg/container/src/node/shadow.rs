use std::fmt::Display;
use std::path::Path;

use common::errors::*;

pub struct PasswdEntry {
    pub name: String,
    pub password: String,
    pub uid: u32,
    pub gid: u32,
    pub comment: String,
    pub directory: String,
    pub shell: String,
}

/// Reads all entries in the local linux /etc/passwd file.
///
/// Returns all users registered in the system.
pub fn read_passwd() -> Result<Vec<PasswdEntry>> {
    let mut out = vec![];
    let data = std::fs::read_to_string("/etc/passwd")?;
    for line in data.lines() {
        let fields = line.split(":").collect::<Vec<_>>();
        if fields.len() != 7 {
            return Err(format_err!(
                "Incorrect number of fields in passwd line: \"{}\"",
                line
            ));
        }

        out.push(PasswdEntry {
            name: fields[0].to_string(),
            password: fields[1].to_string(),
            uid: fields[2].parse()?,
            gid: fields[3].parse()?,
            comment: fields[4].to_string(),
            directory: fields[5].to_string(),
            shell: fields[6].to_string(),
        });
    }

    Ok(out)
}

pub struct GroupEntry {
    pub name: String,
    pub password: String,
    pub id: u32,
    pub user_list: Vec<String>,
}

pub fn read_groups_from_path<P: AsRef<Path>>(path: P) -> Result<Vec<GroupEntry>> {
    let mut out = vec![];
    let data = std::fs::read_to_string::<&std::path::Path>(path.as_ref().into())?;
    for line in data.lines() {
        let fields = line.split(":").collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err(format_err!(
                "Incorrect number of fields in group line: \"{}\"",
                line
            ));
        }

        out.push(GroupEntry {
            name: fields[0].to_string(),
            password: fields[1].to_string(),
            id: fields[2].parse()?,
            user_list: fields[3].split(",").map(|s| s.to_string()).collect(),
        });
    }

    Ok(out)
}

pub fn read_groups() -> Result<Vec<GroupEntry>> {
    read_groups_from_path("/etc/group")
}

#[derive(Debug, Clone)]
pub struct IdRange {
    pub start_id: u32,
    pub count: u32,
}

impl IdRange {
    pub fn contains(&self, id: u32) -> bool {
        id >= self.start_id && id <= self.start_id + self.count
    }
}

impl Display for IdRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {})", self.start_id, self.start_id + self.count)
    }
}

pub struct SubordinateIdRange {
    pub name: String,
    pub ids: IdRange,
}

fn read_subordinate_id_file(path: &str) -> Result<Vec<SubordinateIdRange>> {
    let mut out = vec![];

    let data = std::fs::read_to_string(path)?;
    for line in data.lines() {
        let fields = line.split(":").collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(format_err!(
                "Incorrect number of fields in sub id line: \"{}\"",
                line
            ));
        }

        out.push(SubordinateIdRange {
            name: fields[0].to_string(),
            ids: IdRange {
                start_id: fields[1].parse()?,
                count: fields[2].parse()?,
            },
        });
    }

    Ok(out)
}

pub fn read_subuids() -> Result<Vec<SubordinateIdRange>> {
    read_subordinate_id_file("/etc/subuid")
}

pub fn read_subgids() -> Result<Vec<SubordinateIdRange>> {
    read_subordinate_id_file("/etc/subgid")
}

#[derive(Debug)]
pub struct IdMapping {
    pub id: u32,
    pub new_ids: IdRange,
}

impl Display for IdMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {}", self.id, self.new_ids)
    }
}

pub fn newuidmap(pid: i32, mappings: &[IdMapping]) -> Result<()> {
    newidmap("newuidmap", pid, mappings)
}

pub fn newgidmap(pid: i32, mappings: &[IdMapping]) -> Result<()> {
    newidmap("newgidmap", pid, mappings)
}

// newuidmap <pid> <uid> <loweruid> <count> [ <uid> <loweruid> <count> ] ...
// /usr/bin/newuidmap
fn newidmap(binary: &str, pid: i32, mappings: &[IdMapping]) -> Result<()> {
    let mut args = vec![];
    args.push(pid.to_string());

    for mapping in mappings {
        args.push(mapping.id.to_string());
        args.push(mapping.new_ids.start_id.to_string());
        args.push(mapping.new_ids.count.to_string());
    }

    let mut child = std::process::Command::new(binary).args(&args).spawn()?;
    let status = child.wait()?;
    if !status.success() {
        return Err(format_err!("{} exited with failure: {:?}", binary, status));
    }

    Ok(())
}
