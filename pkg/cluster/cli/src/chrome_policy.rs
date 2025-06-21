

/*
https://www.chromium.org/administrators/linux-quick-start/
https://stackoverflow.com/questions/39023745/google-chrome-automatic-client-certificate-select
https://chromeenterprise.google/policies/?policy=AutoSelectCertificateForUrls

Verify that the policy shows up in
chrome://policies

Check the policy logs on the above page if it doesn't
*/

use common::errors::*;
use file::LocalPath;

use crate::ssh::*;

const CHROME_DIR_OPTIONS: &'static [&'static str] = &[
    "/opt/google/chrome",
    "/etc/opt/chrome",
];

pub async fn setup_chrome_cert_policy(common_name: &str) -> Result<()> {
    let op = LocalOperator::default();
    let op: &dyn MachineOperator = &op;

    println!("Setting up Chrome client certificate policy...");

    {
        let mut found = None;
        for dir in CHROME_DIR_OPTIONS {
            if op.file_exists(dir).await? {
                found = Some(dir);
                break;
            }
        }

        match found {
            Some(v) => v,
            None => {
                println!("=> Unable to find Google Chrome. Skipping...");
                return Ok(())
            }
        }
    };

    // This seems to be used as the base dir for all cases.
    let chrome_dir = "/etc/opt/chrome";

    let policy_path = LocalPath::new(chrome_dir).join("policies/managed/dacha-cluster.policy");
    let policy = r#"
        {
            "AutoSelectCertificateForUrls": [
                "{\"pattern\":\"https://[*.]cluster.internal/*\",\"filter\":{\"SUBJECT\":{\"CN\":\"{SN}\"}}}"
            ]
        }
    "#.replace("{SN}", common_name).replace(' ', "").replace('\n', "");

    if !op.file_exists(&policy_path).await? || op.download_string(&policy_path).await? != policy {
        let dir = policy_path.parent().unwrap();
        op.run(&format!("sudo mkdir -p {}", dir.as_str())).await?;
        op.upload_with(policy.as_bytes(), &policy_path, &UploadOptions::new().sudo()).await?; 
    }

    println!("=> Done");
    Ok(())
}