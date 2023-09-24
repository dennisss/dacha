use std::{sync::Arc, time::Duration};

use common::errors::*;
use google_auth::GoogleRestClient;
use google_discovery_generated::dns_v1;

pub struct Client {
    raw: dns_v1::DnsClient,
    project: String,
}

impl Client {
    pub fn new(project: &str, rest_client: Arc<GoogleRestClient>) -> Result<Self> {
        Ok(Self {
            raw: dns_v1::DnsClient::new(rest_client)?,
            project: project.to_string(),
        })
    }

    /// Sets the value of a TXT DNS record.
    ///
    /// Name should be of the form "x.domain.com.". Returns once the operation
    /// is marked as "done" though extra time may be needed for full propagation
    /// to all Google DNS servers.
    pub async fn set_txt_record<T: AsRef<str>>(
        &self,
        dns_name: &str,
        ttl: i32,
        data: &[T],
    ) -> Result<()> {
        let data = self.encode_txt_rrdata(data)?;

        // Find the zone containing the record.
        let zone_name = {
            let mut zone_name = None;

            let target_zone_dns_name = self.parent_dns_name(dns_name)?;

            let res = self
                .raw
                .managed_zones_list(
                    &self.project,
                    &dns_v1::ManagedZonesListParameters::default(),
                )
                .await?;

            for zone in &res.managedZones {
                if target_zone_dns_name == &zone.dnsName {
                    zone_name = Some(zone.name.clone());
                    break;
                }
            }

            zone_name.ok_or_else(|| {
                format_err!("No zone in project for dns name: {}", target_zone_dns_name)
            })?
        };

        // Check if it already exists.
        let existing_rrset = {
            let mut params = dns_v1::ResourceRecordSetsListParameters::default();
            params.name = dns_name.to_string();
            params.typ = "TXT".to_string();

            let mut res = self
                .raw
                .resource_record_sets_list(&self.project, &zone_name, &params)
                .await?;

            if !res.nextPageToken.is_empty() {
                return Err(err_msg("Unexpected paginated point lookup single lookup"));
            }

            let mut found = false;
            if !res.rrsets.is_empty() {
                Some(res.rrsets.remove(0))
            } else {
                None
            }
        };

        if let Some(rrset) = &existing_rrset {
            if rrset.ttl == ttl && rrset.rrdatas == data {
                return Ok(());
            }
        }

        let mut change = {
            let mut change = dns_v1::Change::default();

            if let Some(rrset) = existing_rrset {
                change.deletions.push(rrset);
            }

            let mut addition = dns_v1::ResourceRecordSet::default();
            addition.name = dns_name.to_string();
            addition.typ = "TXT".to_string();
            addition.ttl = 300;
            addition.rrdatas = data.clone();
            change.additions.push(addition);

            let res = self
                .raw
                .changes_create(
                    &self.project,
                    &zone_name,
                    &change,
                    &dns_v1::ChangesCreateParameters::default(),
                )
                .await?;

            res
        };

        // Refresh change until it is done.
        loop {
            match change.status.as_str() {
                "pending" => {}
                "done" => break,
                _ => return Err(err_msg("Unsupported change status")),
            }

            if change.additions[0].rrdatas != data {
                // In this case, we can't properly diff if a change is done.
                eprintln!(
                    "Inconsistent serialization between client and cloud DNS: {:?} vs {:?}",
                    change.additions[0].rrdatas, data
                );
            }

            executor::sleep(Duration::from_secs(5)).await?;

            change = self
                .raw
                .changes_get(
                    &self.project,
                    &zone_name,
                    &change.id,
                    &dns_v1::ChangesGetParameters::default(),
                )
                .await?;
        }

        Ok(())
    }

    // The Cloud DNS API canonically returns each element of
    fn encode_txt_rrdata<T: AsRef<str>>(&self, data: &[T]) -> Result<Vec<String>> {
        let mut out = vec![];

        for v in data {
            let v = v.as_ref();
            // TODO: Validate there are no quotas that we need to escape.

            out.push(format!("\"{}\"", v));
        }

        Ok(out)
    }

    fn parent_dns_name<'a>(&self, name: &'a str) -> Result<&'a str> {
        if !name.ends_with(".") {
            return Err(err_msg("Malformed DNS name"));
        }

        let (_, rest) = name.split_once(".").unwrap();
        if rest.is_empty() {
            return Err(err_msg("DNS name has no parent"));
        }

        Ok(rest)
    }
}
