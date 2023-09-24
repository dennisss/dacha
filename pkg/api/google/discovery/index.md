https://developers.google.com/discovery/v1/getting_started#rest

GET https://discovery.googleapis.com/discovery/v1/apis


Generates code to interface with Google REST APIs by reading the discovery descriptions. Given that Google internally pulls this info from Protobuf definitions (likely proto3 ones), we define structs which sparsely serialize/deserialize with default values.
