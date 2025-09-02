
use reflection::ParseFrom;

use common::errors::*;
use common::bytes::Bytes;

/// Client that interfaces with the unofficial/internal API used by the McMaster website.
pub struct McMasterWebClient {
    client: http::SimpleClient,
    cookies: String,
}

pub const EMPTY_CURRENTORDER: &'static str = "EMPTY_CURRENTORDER";

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
struct ActivitySummaryData {
    #[parse(name = "OrderIds")]
    pub order_ids: Vec<String>,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct OrderDetailsData {
    #[parse(name = "mView")]
    pub inner: OrderDetailsMView
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct OrderDetailsMView {
    pub EmailPlacedTime: String,
    pub OrderPlacedByVisitorID: u64,
    pub CurrentOrderStatus: String,
    pub InvoiceTotals: InvoiceTotals,
    pub RegNbr: u64,
    pub Title: String,
    pub EmailSubject: String,
    pub PlacedTime: String,
    pub CarrierTrackingInformation: Vec<CarrierTrackingInformation>,
    pub DetailGroups: Vec<DetailGroup>
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct InvoiceTotals {
    pub TotalAmt: f64,
    pub TaxAmt: f64,
    pub TotalMerchandiseAmt: f64,
    pub ShippingAmt: f64,
    pub HazardAmt: f64,
    pub HazardAmtTxt: String,
    pub TotalAmtTxt: String,
    pub TaxAmtTxt: String,
    pub MerchandiseAmtTxt: String,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct CarrierTrackingInformation {
    pub TrackingNbrs: Vec<TrackingNumber>,
    pub CarrierName: String,
    pub CarrierId: u64
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct TrackingNumber {
    pub Url: String,
    pub TrackingNbr: String,
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct DetailGroup {
    pub Title: String,
    pub DtlRows: Vec<DetailRow>
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct DetailRow {
    pub PoTxt: String,
    pub NonbreakingTxt: Vec<String>,
    pub Unit: String,
    pub LnAttr2: String,
    pub Subtitle: String,
    pub TotalPrice: String,
    pub Title: String,
    pub TitleLink: String,
    pub WoLnSeq: f64,
    pub Quantity: String,
    pub LineNbr: String,
    pub QuantityUnit: String,
    pub LnAttr1: String,
    pub PartNbr: String,
    pub UnitPrice: String,
    pub PluralUnit: String,
    pub Reference: String,
    pub MongoOrderId: String,
    pub ImgSrcSet: ImgSrcSet
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ImgSrcSet {
    pub ImgSrc: String,
    pub ImgSrcSet: Vec<ImageSource>
}

#[derive(Parseable, Debug)]
#[parse(allow_unknown = true)]
pub struct ImageSource {
    pub Height: u64,
    pub Width: u64,
    pub ImgSrc: String,
    pub ImgResolution: String,
}


impl McMasterWebClient {
    pub async fn create(cookies: String) -> Result<Self> {
        let client = http::SimpleClient::new(http::SimpleClientOptions::default());

        Ok(Self { client, cookies })
    }


    pub async fn list_orders(&self) -> Result<Vec<String>> {
        let mut req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri("https://www.mcmaster.com/mv1752855842/WebParts/Activity/ActivityPageWebPart.aspx?cntnridtxt=MainContent&actablevelc=cisactive&useEs6=true");

        req = self.add_common_headers(req, false);

        let req = req.build()?;
        
        let res = self.client
            .request(
                &req.head,
                Bytes::new(),
                &http::ClientRequestContext::default(),
            )
            .await?;

        if !res.ok() {
            return Err(format_err!(
                "Request failed: {:?}: {:?}",
                res.head.status_code,
                res.body
            ));
        }

        // The first 10 bytes are a zero padded number (e.g. '0000007843') which specify how many bytes of json data follow it.
        let json_length = {
            if res.body.len() < 10 {
                return Err(err_msg("Unknown data format"));
            }
            
            std::str::from_utf8(&res.body[0..10])?
                .parse::<usize>()?
        };

        let json_data = {
            if res.body.len() < 10 + json_length {
                return Err(err_msg("Missing complete json data"));
            }

            std::str::from_utf8(&res.body[10..(10 + json_length)])?
        };

        let value = json::parse(json_data)?;

        let object = ActivitySummaryData::parse_from(json::ValueParser::new(&value))?;

        Ok(object.order_ids)
    }

    pub async fn get_order_details(&self, order_id: &str) -> Result<OrderDetailsData> {
        let mut req = http::RequestBuilder::new()
            .method(http::Method::GET)
            .uri(&format!("https://www.mcmaster.com/mv1752855842/WebParts/Activity/OrderDetailWebPart.aspx?cntnridtxt=OrderDetailContent&OrderId={}&GroupBy=undefined&payment=true&useEs6=true", order_id));

        req = self.add_common_headers(req, true);

        let req = req.build()?;
        
        let res = self.client
            .request(
                &req.head,
                Bytes::new(),
                &http::ClientRequestContext::default(),
            )
            .await?;

        if !res.ok() {
            return Err(format_err!(
                "Request failed: {:?}: {:?}",
                res.head.status_code,
                res.body
            ));
        }

        let value = json::parse(std::str::from_utf8(&res.body)?)?;

        let object = OrderDetailsData::parse_from(json::ValueParser::new(&value))?;
        Ok(object)
    }

    fn add_common_headers(&self, mut request: http::RequestBuilder, add_tracing: bool) -> http::RequestBuilder {
        request = request
            .header("Cookie", self.cookies.as_str())
            .header("Referer", "https://www.mcmaster.com/")
            .header("Accept", "*/*")
            .header("Accept-Encoding", "gzip, deflate")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("priority", "u=1, i")
            .header("sec-ch-ua", r#""Google Chrome";v="129", "Not=A?Brand";v="8", "Chromium";v="129""#)
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", r#""Linux""#)
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36")
            .header("x-mcm-features", "2d92eb3acf402dbc631c9ca6a02272b8:644,61,458,457,0,10,7,1,11,463,293,4,628,54,55,44,612,654,86,102,258,206,50,51,411,412,409,403,188,189,407,602,39,319,298,696,237,697,454,106,721,377,744,432,430,42,49,434,312,349,93,392,197,355,340,341,342,344,356,357,365,708,474,345,346,347,348,682,296,732,631,617,622,480,479,478,486,231,230,229,139,174,113,256,123,124,422,133,517,52,518,323,336,136,182,177,724,148,185,459,533,424,425,191,101,640,135,711,716,715,176,71,72,695,638,225,723,141,350,351,62,24,138,693,173,725")
            .header("x-requested-with", "XMLHttpRequest");

        if add_tracing {
            /*
            TODO:

            These headers look like:
                x-mcm-ps-id: 859008962
                x-mcm-t-id: 222524e279a849878e0dda23f522f259

            Javascript code for generating these:
                Span ID:  `Math.round(1e9 * Math.random())`
                Trace ID: `Math.round(1e10 * Math.random()).toString(16);`
            */
        }

        request        
    }
}
