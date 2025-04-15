
/*
Very hacky utilities to inject certificates/credentials into the NSS shared db on linux
so that Chrome/Firefox/etc. correctly interop with cluster run servers.

Requirements
    NSS CLI utils must be installed: `sudo apt install libnss3-tools`

References
    https://chromium.googlesource.com/chromium/src.git/+/master/docs/linux/cert_management.md

Useful commands:
    # Init a new db (assuming it doesn't already exist)
    certutil -d $HOME/.pki/nssdb -N --empty-password

    # List all certificates
    certutil -d sql:$HOME/.pki/nssdb -L

    # List all private keys
    certutil -d sql:$HOME/.pki/nssdb -K

    # Print out information on a single certificate
    certutil -d sql:$HOME/.pki/nssdb -L -n <nickname>

    # Deleting a single certificate
    # NOTE: DOES NOT DELETE THE KEY. need '-F' for that
    certutil -d sql:$HOME/.pki/nssdb -D -n <nickname>

    # Create a P12 file combining a certificate + private key.
    openssl pkcs12 -export -out server.p12 -inkey key.pem -in certificate.pem

    # Load in the P12 file
    pk12util -d sql:$HOME/.pki/nssdb -i server.p12 [-n <nickname>]

    # Insert a trusted server certificate (possibly self-signed)
    certutil -d sql:$HOME/.pki/nssdb -A -t "C,," -n <nickname> -i cert.pem
*/

use std::process::Command;

use common::errors::*;
use file::temp::TempDir;
use file::LocalPath;
use crypto::tls::FileCredentialsManager;
use crypto::x509::Certificate;

// TODO: This still seems to be able to read/write to the terminal so maybe spawn with setsid(). 
macro_rules! run {
	($e:expr) => {{
        use std::io::Write;

        let output = $e
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()?;
        if !output.status.success() {
            std::io::stdout().write_all(&output.stdout)?;
            std::io::stderr().write_all(&output.stderr)?;
            return Err(format_err!("Command failed with status: {:?}", output.status));
        }
    }};
}

pub(crate) async fn check_have_nss_utils() -> Result<()> {
    fn check_command(name: &str) -> Result<()> {
        let output = command_args!("which {name}").output()?;
        if !output.status.success() {
            return Err(format_err!("Missing command \"{}\"", name));
        }

        Ok(())
    }

    check_command("certutil")?;
    check_command("pk12util")?;
    check_command("openssl");
    Ok(())
}

/// Given some directory of user credentials, installs them into the local machine's
/// "NSS Shared DB". This will have Chrome/Firefox recognize cluster servers and send
/// the user's identity as a client certificate.
///
/// The expected format of the certificates in the 'manager' is the same as the output
/// of the login command (main user cert + localhost cert).
pub(crate) async fn install_nss_certificates(manager: &mut FileCredentialsManager) -> Result<()> {
    let home = std::env::var("HOME")?;
    let db_path = format!("sql:{}/.pki/nssdb", home);

    let (cert_path, key_path) = manager.certificate_and_pkey_path("").unwrap();
    let p12_path = manager.dir().join("certificate.p12");

    let (localhost_cert_path, _) = manager.certificate_and_pkey_path("localhost").unwrap();

    if file::exists(&p12_path).await? {
        file::remove_file(&p12_path).await?;
    }

    // Delete any existing DB state. This is the easiest way to get rid of any old certificates.
    {
        let db_path = LocalPath::new(&home).join(".pki/nssdb");

        for name in ["cert9.db", "key4.db"] {
            let p = db_path.join(name);
            if file::exists(&p).await? {
                file::remove_file(&p).await?;
            }
        }

        // Re-init
        run!(command_args!("
            certutil -d {db_path.as_str()} -N --empty-password
        "));
    }

    // Create the P12 version of the main client certiticate.
    run!(command_args!("
        openssl pkcs12 -export
        -passout pass:
        -in {cert_path.as_str()}
        -inkey {key_path.as_str()}
        -out {p12_path.as_str()}
    "));

    // Insert the client certificate.
    // TODO: The "user" nickname here seems to be getting ignored.
    run!(command_args!("
        pk12util -d {db_path.as_str()} -i {p12_path.as_str()}
    ").arg("-W").arg(""));

    // Insert all the registry certificate.
    {
        let registry = manager.registry().unwrap();
        let tmp_dir = TempDir::create()?;
        let tmp_file = tmp_dir.path().join("cert.pem");

        for cert in registry.certificates() {
            let name = cert.subject().common_name()?.unwrap();
            file::write(&tmp_file, Certificate::to_pem(&[cert.inner()])).await?;
            run!(command_args!("
                certutil -d {db_path.as_str()} -A -t C,, -i {tmp_file.as_str()} -n {name}
            "));
        }
    }

    println!("New keys/certificates installed in NSS.\nPlease restart Chrome/Firefox\n(e.g. go to chrome://restart)");

    Ok(())
}

