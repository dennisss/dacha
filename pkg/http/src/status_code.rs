

// https://www.iana.org/assignments/http-status-codes/http-status-codes.xhtml

#[derive(Debug)]
pub struct StatusCode(u16);

impl StatusCode {
	pub fn from_u16(v: u16) -> Option<Self> {
		if v < 100 || v >= 600 {
			None
		} else {
			Some(StatusCode(v))
		}
	}

	pub fn as_u16(&self) -> u16 { self.0 }

	pub fn default_reason(&self) -> Option<&'static str> {
		Some(match self.0 {
			100	=> "Continue",
			101	=> "Switching Protocols",
			102	=> "Processing",
			103	=> "Early Hints",
			200	=> "OK",
			201	=> "Created",
			202	=> "Accepted",
			203	=> "Non-Authoritative Information",
			204	=> "No Content",
			205	=> "Reset Content",
			206	=> "Partial Content",
			207	=> "Multi-Status",
			208	=> "Already Reported",
			226	=> "IM Used",
			300	=> "Multiple Choices",
			301	=> "Moved Permanently",
			302	=> "Found",
			303	=> "See Other",
			304	=> "Not Modified",
			305	=> "Use Proxy",
			307	=> "Temporary Redirect",
			308	=> "Permanent Redirect",
			400	=> "Bad Request",
			401	=> "Unauthorized",
			402	=> "Payment Required",
			403	=> "Forbidden",
			404	=> "Not Found",
			405	=> "Method Not Allowed",
			406	=> "Not Acceptable",
			407	=> "Proxy Authentication Required",
			408	=> "Request Timeout",
			409	=> "Conflict",
			410	=> "Gone",
			411	=> "Length Required",
			412	=> "Precondition Failed",
			413	=> "Payload Too Large",
			414	=> "URI Too Long",
			415	=> "Unsupported Media Type",
			416	=> "Range Not Satisfiable",
			417	=> "Expectation Failed",
			421	=> "Misdirected Request",
			422	=> "Unprocessable Entity",
			423	=> "Locked",
			424	=> "Failed Dependency",
			425	=> "Too Early",
			426	=> "Upgrade Required",
			428	=> "Precondition Required",
			429	=> "Too Many Requests",
			431	=> "Request Header Fields Too Large",
			451	=> "Unavailable For Legal Reasons",
			500	=> "Internal Server Error",
			501	=> "Not Implemented",
			502	=> "Bad Gateway",
			503	=> "Service Unavailable",
			504	=> "Gateway Timeout",
			505	=> "HTTP Version Not Supported",
			506	=> "Variant Also Negotiates",
			507	=> "Insufficient Storage",
			508	=> "Loop Detected",
			510	=> "Not Extended",
			511	=> "Network Authentication Required",
			_ => { return None; }
		})
	}

}

// Continue = 100,
pub const CONTINUE: StatusCode = StatusCode(100);
// 101	Switching Protocols,
pub const SWITCHING_PROTOCOLS: StatusCode = StatusCode(101);
// 102	Processing,
pub const PROCESSING: StatusCode = StatusCode(102);
// 103	Early Hints,
pub const EARLY_HINTS: StatusCode = StatusCode(103);
// 200	OK,
pub const OK: StatusCode = StatusCode(200);
// 201	Created,
pub const CREATED: StatusCode = StatusCode(201);
// 202	Accepted,
pub const ACCEPTED: StatusCode = StatusCode(202);
// 203	Non-Authoritative Information,
pub const NON_AUTHORITATIVE_INFO: StatusCode = StatusCode(203);
// 204	No Content,
pub const NO_CONTENT: StatusCode = StatusCode(204);
// 205	Reset Content,
pub const RESET_CONTENT: StatusCode = StatusCode(205);
// 206	Partial Content,
pub const PARTIAL_CONTENT: StatusCode = StatusCode(206);
// 207	Multi-Status,
pub const MULTI_STATUS: StatusCode = StatusCode(207);
// 208	Already Reported,
pub const ALREADY_REPORTED: StatusCode = StatusCode(208);
// 226	IM Used,
pub const IM_USED: StatusCode = StatusCode(226);
// 300	Multiple Choices,
pub const MULTIPLE_CHOICES: StatusCode = StatusCode(300);
// 301	Moved Permanently,
pub const MOVED_PERMANENTLY: StatusCode = StatusCode(301);
// 302	Found,
pub const FOUND: StatusCode = StatusCode(302);
// 303	See Other,
pub const SEE_OTHER: StatusCode = StatusCode(303);
// 304	Not Modified	[RFC7232, Section 4.1]
pub const NOT_MODIFIED: StatusCode = StatusCode(304);
// 305	Use Proxy	[RFC7231, Section 6.4.5]
pub const USE_PROXY: StatusCode = StatusCode(305);
// 307	Temporary Redirect	[RFC7231, Section 6.4.7]
pub const TEMPORARY_REDIRECT: StatusCode = StatusCode(307);
// 308	Permanent Redirect	[RFC7538]
pub const PERMANENT_REDIRECT: StatusCode = StatusCode(408);
// 400	Bad Request	[RFC7231, Section 6.5.1]
pub const BAD_REQUEST: StatusCode = StatusCode(400);
// 401	Unauthorized	[RFC7235, Section 3.1]
pub const UNAUTHORIZED: StatusCode = StatusCode(401);
// 402	Payment Required	[RFC7231, Section 6.5.2]
pub const PAYMENT_REQUIRED: StatusCode = StatusCode(402);
// 403	Forbidden	[RFC7231, Section 6.5.3]
pub const FORBIDDEN: StatusCode = StatusCode(403);
// 404	Not Found	[RFC7231, Section 6.5.4]
pub const NOT_FOUND: StatusCode = StatusCode(404);
// 405	Method Not Allowed	[RFC7231, Section 6.5.5]
pub const METHOD_NOT_ALLOWED: StatusCode = StatusCode(405);
// 406	Not Acceptable	[RFC7231, Section 6.5.6]
pub const NOT_ACCEPTABLE: StatusCode = StatusCode(406);
// 407	Proxy Authentication Required	[RFC7235, Section 3.2]
pub const PROXY_AUTHENTICATION_REQUIRED: StatusCode = StatusCode(407);
// 408	Request Timeout	[RFC7231, Section 6.5.7]
pub const REQUEST_TIMEOUT: StatusCode = StatusCode(408);
// 409	Conflict	[RFC7231, Section 6.5.8]
pub const CONFLICT: StatusCode = StatusCode(409);
// 410	Gone	[RFC7231, Section 6.5.9]
pub const GONE: StatusCode = StatusCode(410);
// 411	Length Required	[RFC7231, Section 6.5.10]
pub const LENGTH_REQUIRED: StatusCode = StatusCode(411);
// 412	Precondition Failed	[RFC7232, Section 4.2][RFC8144, Section 3.2]
pub const PRECONDITION_FAILED: StatusCode = StatusCode(412);
// 413	Payload Too Large	[RFC7231, Section 6.5.11]
pub const PAYLOAD_TOO_LARGE: StatusCode = StatusCode(413);
// 414	URI Too Long	[RFC7231, Section 6.5.12]
pub const URI_TOO_LONG: StatusCode = StatusCode(414);
// 415	Unsupported Media Type	[RFC7231, Section 6.5.13][RFC7694, Section 3]
pub const UNSUPPORTED_MEDIA_TYPE: StatusCode = StatusCode(415);
// 416	Range Not Satisfiable	[RFC7233, Section 4.4]
pub const RANGE_NOT_SATISFIABLE: StatusCode = StatusCode(416);
// 417	Expectation Failed	[RFC7231, Section 6.5.14]
pub const EXPECTATION_FAILED: StatusCode = StatusCode(417);
// 421	Misdirected Request	[RFC7540, Section 9.1.2]
pub const MISDIRECTED_REQUEST: StatusCode = StatusCode(421);
// 422	Unprocessable Entity	[RFC4918]
pub const UNPROCESSABLE_ENTITY: StatusCode = StatusCode(422);
// 423	Locked	[RFC4918]
pub const LOCKED: StatusCode = StatusCode(423);
// 424	Failed Dependency	[RFC4918]
pub const FAILED_DEPENDENCY: StatusCode = StatusCode(424);
// 425	Too Early	[RFC8470]
pub const TOO_EARLY: StatusCode = StatusCode(425);
// 426	Upgrade Required	[RFC7231, Section 6.5.15]
pub const UPGRADE_REQUIRED: StatusCode = StatusCode(426);
// 428	Precondition Required	[RFC6585]
pub const PRECONDITION_REQUIRED: StatusCode = StatusCode(428);
// 429	Too Many Requests	[RFC6585]
pub const TOO_MANY_REQUESTS: StatusCode = StatusCode(429);
// 431	Request Header Fields Too Large	[RFC6585]
pub const REQUEST_HEADER_FIELDS_TOO_LARGE: StatusCode = StatusCode(431);
// 451	Unavailable For Legal Reasons	[RFC7725]
pub const UNAVAILABLE_FOR_LEGAL_REASONS: StatusCode = StatusCode(451);
// 500	Internal Server Error	[RFC7231, Section 6.6.1]
pub const INTERNAL_SERVER_ERROR: StatusCode = StatusCode(500);
// 501	Not Implemented	[RFC7231, Section 6.6.2]
pub const NOT_IMPLEMENTED: StatusCode = StatusCode(501);
// 502	Bad Gateway	[RFC7231, Section 6.6.3]
pub const BAD_GATEWAY: StatusCode = StatusCode(502);
// 503	Service Unavailable	[RFC7231, Section 6.6.4]
pub const SERVICE_UNAVAILABLE: StatusCode = StatusCode(503);
// 504	Gateway Timeout	[RFC7231, Section 6.6.5]
pub const GATEWAY_TIMEOUT: StatusCode = StatusCode(504);
// 505	HTTP Version Not Supported	[RFC7231, Section 6.6.6]
pub const HTTP_VERSION_NOTSUPPORTED: StatusCode = StatusCode(505);
// 506	Variant Also Negotiates	[RFC2295]
pub const VARIANT_ALSO_NEGOTIATES: StatusCode = StatusCode(506);
// 507	Insufficient Storage	[RFC4918]
pub const INSUFFICIENT_STORAGE: StatusCode = StatusCode(507);
// 508	Loop Detected	[RFC5842]
pub const LOOP_DETECTED: StatusCode = StatusCode(508);
// Not Extended	[RFC2774]
pub const NOT_EXTENDED: StatusCode = StatusCode(510);
/// Network Authentication Required	[RFC6585]
pub const NETWORK_AUTHENTICATION_REQUIRED: StatusCode = StatusCode(511);
