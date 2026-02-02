
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
            .uri("https://www.mcmaster.com/mv1769032333/WebParts/OrderHistory/ActivityPageWebPart.aspx?cntnridtxt=MainContent&actablevelc=cisactive&useEs6=true");

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
            .uri(&format!("https://www.mcmaster.com/mv1769032333/WebParts/OrderHistory/OrderDetailWebPart.aspx?cntnridtxt=OrderDetailContent&OrderId={}&GroupBy=null&useEs6=true", order_id));

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
            .header("x-mcm-features", "91f36cc5cee021e1fbb4da780cbf6e63:482,666,63,475,0,11,7,1,12,477,333,4,644,58,59,45,621,673,88,111,296,241,54,55,443,442,439,441,607,40,359,338,715,274,717,472,116,742,415,767,449,52,451,352,385,98,431,234,394,378,380,395,396,404,727,484,381,382,383,384,702,336,755,649,627,636,271,270,269,167,206,127,294,139,140,154,513,56,514,363,157,220,177,226,531,448,229,109,660,208,663,263,168,386,387,388,64,24,530,160,748,141,142,500,282,552,550,554,745,735,710,616,655,100,171,219,210,222,525,292,120,217,211,152,265,705,465,645,209")
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
