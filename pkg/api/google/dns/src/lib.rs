use std::{sync::Arc, time::Duration};

use base_error::*;
use google_auth::GoogleRestClient;
use google_discovery_generated::dns_v1;
use net::ip::IPAddress;

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
    ) -> Result<bool> {
        let data = self.encode_txt_rrdata(data)?;

        let mut record = dns_v1::ResourceRecordSet::default();
        record.name = dns_name.to_string();
        record.typ = "TXT".to_string();
        record.ttl = ttl;
        record.rrdatas = data.clone();

        self.update_record(record).await
    }

    pub async fn set_address_records(
        &self,
        dns_name: &str,
        ttl: i32,
        ips: &[IPAddress],
    ) -> Result<bool> {
        let mut changed = false;

        let mut a_record = dns_v1::ResourceRecordSet::default();
        a_record.name = dns_name.to_string();
        a_record.typ = "A".to_string();
        a_record.ttl = ttl;

        let mut aaaa_record = dns_v1::ResourceRecordSet::default();
        aaaa_record.name = dns_name.to_string();
        aaaa_record.typ = "AAAA".to_string();
        aaaa_record.ttl = ttl;

        for ip in ips {
            if ip.is_v4() {
                a_record.rrdatas.push(ip.to_string());
            } else {
                aaaa_record.rrdatas.push(ip.to_string());
            }
        }

        Ok(
            self.update_record(a_record).await? || 
            self.update_record(aaaa_record).await?
        )
    }

    async fn update_record(&self, rrset: dns_v1::ResourceRecordSet) -> Result<bool> {
        if !rrset.name.ends_with(".") {
            return Err(format_err!("DNS name should end with a dot: {}", rrset.name));
        }

        // Find the zone containing the record.
        let zone_name = {
            let mut zone_name = None;

            let res = self
                .raw
                .managed_zones_list(
                    &self.project,
                    &dns_v1::ManagedZonesListParameters::default(),
                )
                .await?;

            for zone in &res.managedZones {
                if rrset.name == zone.dnsName || rrset.name.ends_with(&format!(".{}", zone.dnsName)) {
                    zone_name = Some(zone.name.clone());
                    break;
                }
            }

            zone_name.ok_or_else(|| {
                format_err!("No zone in project for dns name: {}", rrset.name)
            })?
        };

        // Check if it already exists.
        let existing_rrset = {
            let mut params = dns_v1::ResourceRecordSetsListParameters::default();
            params.name = rrset.name.clone();
            params.typ = rrset.typ.clone();

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

        if let Some(existing_rrset) = &existing_rrset {
            // TODO: Check all fields.
            if existing_rrset.ttl == rrset.ttl && existing_rrset.rrdatas == rrset.rrdatas {
                return Ok(false);
            }
        } else {
            if rrset.rrdatas.is_empty() {
                return Ok(false);
            }
        }

        let mut change = {
            let mut change = dns_v1::Change::default();

            if let Some(rrset) = existing_rrset {
                change.deletions.push(rrset);
            }

            if !rrset.rrdatas.is_empty() {
                change.additions.push(rrset);
            }

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

            /*
            if change.additions[0].rrdatas != data {
                // In this case, we can't properly diff if a change is done.
                eprintln!(
                    "Inconsistent serialization between client and cloud DNS: {:?} vs {:?}",
                    change.additions[0].rrdatas, rrset.rrdatas
                );
            }
            */

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

        Ok(true)
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
}
