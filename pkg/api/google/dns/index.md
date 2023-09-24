
JSON representations of different DNS record types (how they are packed into rrdatas) is described in https://cloud.google.com/dns/docs/reference/json-record.


Example request for adding a zone:

```
POST https://dns.googleapis.com/dns/v1beta2/projects/dacha-main/managedZones
{
  "cloudLoggingConfig": {
    "enableLogging": false
  },
  "description": "",
  "dnsName": "dacha.dev.",
  "dnssecConfig": {
    "state": "ON"
  },
  "name": "my-zone-dame",
  "visibility": "PUBLIC"
}
```

Simple example of adding an A record:

```
POST https://dns.googleapis.com/dns/v1beta2/projects/dacha-main/managedZones/my-zone/changes
{
  "additions": [
    {
      "name": "www.dacha.page.",
      "type": "A",
      "ttl": 300,
      "rrdata": [
        "192.168.0.1"
      ]
    }
  ]
}
```


ns-cloud-c1.googledomains.com.
ns-cloud-c2.googledomains.com.
ns-cloud-c3.googledomains.com.
ns-cloud-c4.googledomains.com.
