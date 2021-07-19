extern crate common;
extern crate container;
extern crate protobuf;

use common::errors::*;
use protobuf::text::parse_text_proto;

async fn run() -> Result<()> {
    let rt = container::ContainerRuntime::create().await?;

    let mut container_config = container::ContainerConfig::default();

/*
    std::fs::create_dir(root_dir.join("proc"))?;
    // std::fs::create_dir(root_dir.join("bin"))?;
    std::fs::create_dir_all(root_dir.join("usr/lib"))?;
    std::fs::create_dir_all(root_dir.join("usr/bin"))?;
    std::fs::create_dir_all(root_dir.join("lib64"))?;

    nix::mount::mount::<Path, _, _, Path>(
        None, &root_dir.join("proc"), Some("proc"), 
        MsFlags::MS_NOEXEC | MsFlags::MS_NOSUID | MsFlags::MS_NODEV, None)?;
    
    nix::mount::mount::<_, _, str, str>(
        Some("/usr/bin"), &root_dir.join("usr/bin"), None, MsFlags::MS_BIND | MsFlags::MS_RDONLY, None)?;

    nix::mount::mount::<_, _, str, str>(
        Some("/lib64"), &root_dir.join("lib64"), None, MsFlags::MS_BIND | MsFlags::MS_RDONLY, None)?;

    nix::mount::mount::<str, Path, Path, Path>(
        Some("/usr/lib"), &root_dir.join("usr/lib"), None, MsFlags::MS_BIND | MsFlags::MS_RDONLY, None)?;

    */

    parse_text_proto(r#"
        process {
            args: ["/usr/bin/cat", "Hello world!"]
        }
        mounts: [
            {
                destination: "/proc"
                type: "proc"
                source: "proc"
                options: ["noexec", "nosuid", "nodev"]
            },
            {
                destination: "/usr/bin"
                source: "/usr/bin"
                options: ["bind", "ro"]
            },
            {
                destination: "/lib64"
                source: "/lib64"
                options: ["bind", "ro"]
            },
            {
                destination: "/usr/lib"
                source: "/usr/lib"
                options: ["bind", "ro"]
            }
        ]
    "#, &mut container_config)?;

    rt.start(&container_config).await;

    rt.run().await?;

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}