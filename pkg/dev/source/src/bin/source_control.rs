// NOTE: This script currently assumes that files aren't being modified while it
// is running.

// cargo run --bin source_control -- add 'testdata/video/**'

/*
TODO: NEed a git pre-commit to block committing if there are unregistered changes to any tracked large files.

TODO: Need to respect any gitignores while scanning through files.

TODO: Ensure that no files are simultaneously tracked by git and external_files.pbtxt (also report if any large files)

TODO: Integrate with the image linter to see if we need to sanitize any images before uploading them.

We will store a file called external_files.pbtxt
    files [{
        path: "testdata/image.png"
        size: 1234
        is_directory: false #
        mirrors: [
            { url: "gs://da-source-blobs/sha256/FFF...FFFF" }
        ]
    }]

Also need a concept of protected / immutable files (for keeping track of specific versions of things).

Need a pre-commit hook to block adding any files > 10KB

General workflow:
- User creates a binary file
- User registers it via `cargo run --bin source_control -- add path/to/file.bin`
    - This needs to update the .gitignore and
- User commits using regular git.
    - Need a pre-commit to verify things are in sync

- User modifies one of the files
- User runs `cargo run --bin source_control -- update`
    - This scans all existing files and possibly re-pushes them

- If the user needs to la
- User

*/

// ".git/info/exclude" will contain the full lsit.

#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use common::{errors::*, io::Readable, line_builder::LineBuilder};
use crypto::{hasher::Hasher, sha256::SHA256Hasher};
use dev_source_proto::dev::*;
use file::{GlobIterator, LocalPath, LocalPathBuf};
use google_auth::GoogleServiceAccount;

const EXTERNAL_FILES_PROTO_PATH: &'static str = "external_files.pbtxt";
const GIT_EXCUDE_FILE_PATH: &'static str = ".git/info/exclude";

async fn list_big_files() -> Result<()> {
    // file::recursively_list_dir(dir, callback)

    Ok(())
}

#[derive(Args)]
struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    /// Stages some files from the local workspace into the version controlled
    /// repository.
    ///
    /// - If the files are already in the repository, then we will check for
    ///   diffs and record the updated file contents.
    /// - Similarly files removed from the local workspace
    #[arg(name = "add")]
    Add(AddCommand),

    /// Downloads all external tracked files into the local workspace
    /// -> Skips any files that already exist).
    #[arg(name = "fetch")]
    Fetch,
    // TODO: Need a command to verify that all files in cloud storage have the correct contents
    // and hash.
}

#[derive(Args)]
struct AddCommand {
    /// Glob path to use for matching files to stage.
    /// Omitting this will any changes for already tracked files.
    #[arg(positional = true)]
    path: String,
}

async fn load_external_files_proto() -> Result<ExternalSourceFiles> {
    let path = project_path!(EXTERNAL_FILES_PROTO_PATH);

    let mut value = ExternalSourceFiles::default();

    if file::exists(&path).await? {
        let text_value = file::read(&path).await?;
        protobuf::text::parse_text_proto(std::str::from_utf8(&text_value)?, &mut value)?;
    }

    Ok(value)
}

async fn save_external_files_proto(mut value: ExternalSourceFiles) -> Result<()> {
    let path = project_path!(EXTERNAL_FILES_PROTO_PATH);

    value
        .files_mut()
        .sort_by_key(|file| file.path().to_string());

    // TODO: Serialize with the protobuf header
    let text_value = protobuf::text::serialize_text_proto(&value);

    file::write(&path, &text_value).await?;

    // Apply any changed to git excluded files.
    {
        let mut lines = LineBuilder::new();
        for file in value.files() {
            lines.add(file.path());
        }

        file::write(project_path!(GIT_EXCUDE_FILE_PATH), lines.to_string()).await?;
    }

    Ok(())
}

/// Maximum number of files we can add in one 'add' command. This is setup
///
/// TODO: Implement me.
const MAX_ADDED_FILES: usize = 200;

async fn run_add_command(cmd: AddCommand) -> Result<()> {
    let base_dir = file::project_dir();
    let pattern = base_dir.join(LocalPath::new(&cmd.path));

    let mut glob = GlobIterator::create(&pattern)?;

    let git_index = git::read_index().await?;
    let git_entries = {
        let mut m = HashMap::new();

        for entry in &git_index.entries {
            m.insert(entry.name.as_str(), entry);
        }

        m
    };

    /////////////////////////////////////////////
    // Step 1: Loading our index of files already tracked in the database.

    let mut external_files = load_external_files_proto().await?;

    // TODO: Normalize the capitalization of hash hex strings.

    // Map of existing file path to the hash of that file.
    let mut path_to_hash = HashMap::<&str, &str>::default();
    // Map of existing file hashes in the dataset.
    let mut existing_hashes = HashSet::new();

    for file in external_files.files() {
if git_entries.contains_key(file.path()) {
            return Err(err_msg(
                "A file tracked by git was found in the external_files map",
            ));
        }

        existing_hashes.insert(file.sha256_sum());

        let path = base_dir.join(file.path());
        if glob.matches_file(&path) {
            path_to_hash.insert(file.path(), file.sha256_sum());
        }
    }

    // New data blobs to need to be uploaded.
    // Map of hash to local file from which to read it.
    let mut data_to_upload = HashMap::<String, LocalPathBuf>::new();

    // Files to remove from the repository. Also includes updated files.
    let mut removed_files = HashSet::new();

    // New files to add.
    let mut added_files = vec![];

    /////////////////////////////////////////////
    // Step 2: Scan matching files in the repository.

    while let Some(path) = glob.next().await? {
        let rel_path = match path.strip_prefix(&base_dir) {
            Some(v) => v,
            None => continue,
        };

        // submodules are just tracked as a directory so we need to exclude everything
        // in the directory.
        {
            let mut found = false;
            let mut cur_path = Some(rel_path);
            while let Some(p) = cur_path.take() {
                // TODO: Apply the same normalization to the git index.
                if git_entries.contains_key(p.as_str().strip_suffix("/").unwrap_or(p.as_str())) {
                    found = true;
                    break;
                }

                cur_path = p.parent();
            }

            if found {
            continue;
            }
        }

        let meta = file::metadata(&path).await?;
        if meta.is_dir() {
            continue;
        }

        println!("{}", rel_path.as_str());

        // TODO: Ideally we should mtimes to avoid re-calculating the hashes for
        // unchanged files.
        let hash = {
            let mut file = file::LocalFile::open(&path)?;
            let mut hasher = SHA256Hasher::default();

            let mut block = vec![0u8; 8192];

            loop {
                let n = file.read(&mut block).await?;
                if n == 0 {
                    break;
                }

                hasher.update(&block[0..n]);
            }

            base_radix::hex_encode(&hasher.finish())
        };

        println!("=> SHA256: {}", hash);

        if let Some(existing_hash) = path_to_hash.remove(rel_path.as_str()) {
            if existing_hash == &hash {
                println!("=> No diff");
                continue;
            } else {
                println!("=> Diff");
                removed_files.insert(rel_path.as_str().to_string());
            }
        } else {
            println!("=> New");
        }

        let mut new_entry = ExternalSourceFile::default();
        new_entry.set_path(rel_path.as_str());
        new_entry.set_size(meta.len());
        new_entry.set_sha256_sum(&hash);
        added_files.push(new_entry);

        if !existing_hashes.contains(&hash.as_str()) && !data_to_upload.contains_key(&hash) {
            data_to_upload.insert(hash, path);
        }
    }

    for path in path_to_hash.keys() {
        println!("{}", path);
        println!("=> REMOVE");
        removed_files.insert(path.to_string());
    }

    if data_to_upload.is_empty() && removed_files.is_empty() && added_files.is_empty() {
        println!("[No changes]");
        return Ok(());
    }

    /////////////////////////////////////////////
    // Step 3: Ask for user confirmation

    println!("");
    println!("Continue: [y/N]?");
    if !file::read_user_confirmation().await? {
        println!("[Exit without changing anything]");
        return Ok(());
    }

    /////////////////////////////////////////////
    // Step 4: Upload all new blobs.

    println!("Uploading new blobs...");

    {
        let data =
            file::read_to_string("/home/dennis/.credentials/dacha-main-748d2acba112.json").await?;

        let sa: Arc<GoogleServiceAccount> =
            Arc::new(google_auth::GoogleServiceAccount::parse_json(&data)?);

        let rest_client = Arc::new(google_auth::GoogleRestClient::create(sa.clone())?);
        let client = google_storage::Client::new(rest_client)?;

        for (hash, path) in data_to_upload {
            println!("=> Uploading: {}", hash);

            let object_name = format!("sha256/{}", hash);
            let data = http::static_file_handler::StaticFileBody::open(&path).await?;
            client
                .upload("da-sources", &object_name, Box::new(data))
                .await?;
        }
    }

    /////////////////////////////////////////////
    // Step 5: Remove old file entries
    {
        let mut i = 0;
        while i < external_files.files_len() {
            if removed_files.remove(external_files.files()[i].path()) {
                external_files.files_mut().swap_remove(i);
                continue;
            }

            i += 1;
        }

        assert!(removed_files.is_empty());
    }

    /////////////////////////////////////////////
    // Step 6: Add new file entries
    for entry in added_files {
        external_files.add_files(entry);
    }

    /////////////////////////////////////////////
    // Step 7: Write to registry.

    save_external_files_proto(external_files).await?;

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    match args.command {
        Command::Add(cmd) => {
            run_add_command(cmd).await?;
        }
        Command::Fetch => todo!(),
    }

    Ok(())
}
