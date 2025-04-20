// Utilities for using SSH to operate a remote server.

use std::io::Write;
use std::process::{Command, Stdio};

use common::errors::*;
use file::LocalPath;

#[async_trait]
pub trait MachineOperator: Send + Sync + 'static {
    /// Runs a bash command on the machine and returns the stdout.
    async fn run(&self, command: &str) -> Result<Vec<u8>>;

    async fn file_exists_impl(&self, remote_path: &LocalPath) -> Result<bool>;

    async fn create_dir_all_impl(&self, remote_path: &LocalPath) -> Result<()>;

    async fn upload_file_impl(&self, local_path: &LocalPath, remote_path: &LocalPath)
        -> Result<()>;

    async fn upload_impl(&self, data: &[u8], remote_path: &LocalPath) -> Result<()>;

    async fn download_file_impl(
        &self,
        remote_path: &LocalPath,
        local_path: &LocalPath,
    ) -> Result<()>;

    async fn download_impl(&self, remote_path: &LocalPath) -> Result<Vec<u8>>;
}

impl dyn MachineOperator {
    pub async fn file_exists<P: AsRef<LocalPath> + Send>(&self, remote_path: P) -> Result<bool> {
        self.file_exists_impl(remote_path.as_ref()).await
    }

    pub async fn create_dir_all<P: AsRef<LocalPath> + Send>(&self, remote_path: P) -> Result<()> {
        self.create_dir_all_impl(remote_path.as_ref()).await
    }

    pub async fn upload_file<P: AsRef<LocalPath> + Send, P2: AsRef<LocalPath> + Send>(
        &self,
        local_path: P,
        remote_path: P2,
    ) -> Result<()> {
        self.upload_file_impl(local_path.as_ref(), remote_path.as_ref())
            .await
    }

    pub async fn upload<P: AsRef<LocalPath> + Send>(
        &self,
        data: &[u8],
        remote_path: P,
    ) -> Result<()> {
        self.upload_impl(data, remote_path.as_ref()).await
    }

    pub async fn download_file<P: AsRef<LocalPath> + Send, P2: AsRef<LocalPath> + Send>(
        &self,
        remote_path: P,
        local_path: P2,
    ) -> Result<()> {
        self.download_file_impl(remote_path.as_ref(), local_path.as_ref())
            .await
    }

    pub async fn download<P: AsRef<LocalPath> + Send>(&self, remote_path: P) -> Result<Vec<u8>> {
        self.download_impl(remote_path.as_ref()).await
    }

    pub async fn download_string<P: AsRef<LocalPath> + Send>(&self, remote_path: P) -> Result<String> {
        Ok(String::from_utf8(self.download_impl(remote_path.as_ref()).await?)?)
    }
}

#[derive(Default)]
pub struct LocalOperator {}

#[async_trait]
impl MachineOperator for LocalOperator {
    async fn run(&self, command: &str) -> Result<Vec<u8>> {
        let output = Command::new("/bin/bash").args(["-c", command]).output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }

        Ok(output.stdout)
    }

    async fn file_exists_impl(&self, remote_path: &LocalPath) -> Result<bool> {
        file::exists(remote_path).await
    }

    async fn create_dir_all_impl(&self, remote_path: &LocalPath) -> Result<()> {
        file::create_dir_all(remote_path).await
    }

    async fn upload_file_impl(
        &self,
        local_path: &LocalPath,
        remote_path: &LocalPath,
    ) -> Result<()> {
        file::copy(local_path, remote_path).await?;

        // Propagate executable bits.
        // TODO: Improve this algorithm.
        let mut perms = file::metadata(local_path).await?.permissions();
        if perms.mode() & 0b1 != 0 {
            let mut new_perms = file::metadata(remote_path).await?.permissions();
            new_perms.set_mode(new_perms.mode() | (1 | (1 << 3) | (1 << 6)));
            file::set_permissions(remote_path, new_perms).await?;
        }

        Ok(())
    }

    async fn upload_impl(&self, data: &[u8], remote_path: &LocalPath) -> Result<()> {
        file::write(remote_path, data).await
    }

    async fn download_file_impl(
        &self,
        remote_path: &LocalPath,
        local_path: &LocalPath,
    ) -> Result<()> {
        file::copy(remote_path, local_path).await
    }

    async fn download_impl(&self, remote_path: &LocalPath) -> Result<Vec<u8>> {
        file::read(remote_path).await
    }
}

pub struct SSHClient {
    user: String,
    addr: String,
    args: Vec<String>,
}

impl SSHClient {
    pub fn new(addr: &str, user: &str, args: Vec<String>) -> Self {
        // TODO: validate that addr is an ip address.

        // TODO: Validate 'user' looks like a user naem (no @)

        Self {
            user: user.to_string(),
            addr: addr.to_string(),
            args,
        }
    }

    /// Runs a command on the remote server.
    ///
    /// Returns the stdout result if successful.
    fn run_impl(&self, command: &str) -> Result<Vec<u8>> {
        let mut args = vec![];
        args.push(format!("{}@{}", self.user, self.addr));
        args.extend(self.args.clone());
        args.push(command.to_string());

        let output = Command::new("ssh").args(args).output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }

        Ok(output.stdout)
    }

    fn run_scp(&self, source: &str, destination: &str) -> Result<()> {
        let mut args = vec![];
        args.extend(self.args.clone());
        args.push(source.to_string());
        args.push(destination.to_string());

        let output = Command::new("scp").args(args).output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }

        Ok(())
    }

    fn file_exists_in_dir(&self, dir: &LocalPath, file_name: &str) -> Result<bool> {
        let contents = String::from_utf8(self.run_impl(&format!("ls {}", dir.as_str()))?)?;
        for line in contents.lines() {
            let line = line.trim();
            if line == file_name {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[async_trait]
impl MachineOperator for SSHClient {
    async fn run(&self, command: &str) -> Result<Vec<u8>> {
        self.run_impl(command)
    }

    async fn file_exists_impl(&self, remote_path: &LocalPath) -> Result<bool> {
        // NOTE: We want to differentiate between a file appearing to not
        // exist due to permission issues or the parent directory not existing
        // and actually not existing. So we ls each parent directory until we
        // can't any more to dodge these issues.

        let remote_path = remote_path.normalized();
        if !remote_path.is_absolute() {
            return Err(err_msg("Only absolute paths are supported"));
        }

        let mut dir = file::LocalPathBuf::from("/");

        for segment in remote_path.segments() {
            match segment {
                file::LocalPathSegment::Root => {}
                file::LocalPathSegment::File(name) => {
                    if !self.file_exists_in_dir(&dir, name)? {
                        return Ok(false);
                    }
                    
                    dir.push(name);
                }
                _ => todo!()
            }
        }

        Ok(true)
    }

    async fn create_dir_all_impl(&self, remote_path: &LocalPath) -> Result<()> {
        self.run_impl(&format!("mkdir -p {}", remote_path.as_str()))?;
        Ok(())
    }

    async fn upload_file_impl(
        &self,
        local_path: &LocalPath,
        remote_path: &LocalPath,
    ) -> Result<()> {
        self.run_scp(
            local_path.as_str(),
            &format!("{}@{}:{}", self.user, self.addr, remote_path.as_str()),
        )
    }

    async fn upload_impl(&self, data: &[u8], remote_path: &LocalPath) -> Result<()> {
        let command = format!("cp --no-preserve=all /dev/stdin {}", remote_path.as_str());

        let mut args = vec![];
        args.push(format!("{}@{}", self.user, self.addr));
        args.extend(self.args.clone());
        args.push(command);

        let mut child = Command::new("ssh")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(data)?;
        drop(stdin);

        let output = child.wait_with_output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout).unwrap();
            std::io::stderr().write_all(&output.stderr).unwrap();
            return Err(err_msg("Command failed"));
        }

        Ok(())
    }

    async fn download_file_impl(
        &self,
        remote_path: &LocalPath,
        local_path: &LocalPath,
    ) -> Result<()> {
        self.run_scp(
            &format!("{}@{}:{}", self.user, self.addr, remote_path.as_str()),
            local_path.as_str(),
        )
    }

    // TODO: Figure out if this works with binary files.
    async fn download_impl(&self, remote_path: &LocalPath) -> Result<Vec<u8>> {
        self.run_impl(&format!("cat {}", remote_path.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[testcase]
    async fn ssh_file_exists() -> Result<()> {
        // TODO: Make a hermetic version of this with mock command calls.

        let op = SSHClient::new("10.2.0.1", "cluster-user", vec![
            "-i".into(),
            "~/.ssh/id_cluster".into()
        ]);

        let op: &dyn MachineOperator = &op;

        assert_eq!(op.file_exists("/").await?, true);
        assert_eq!(op.file_exists("/nonexistent").await?, false);
        assert_eq!(op.file_exists("/proc/cpuinfo").await?, true);
        // Will error out
        // assert_eq!(op.file_exists("/root/somefile").await?, true);

        assert_eq!(op.file_exists("/etc/machine-id").await?, true);
        assert_eq!(op.file_exists("/etc/dir/a/b/c").await?, false);

        Ok(())
    }



}

