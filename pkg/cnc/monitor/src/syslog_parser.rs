use base_error::*;

// NOTE: THis assumes there is no structured data.
regexp!(SYSLOG_HEADER_PATTERN => "^<([0-9]+)>([1-9][0-9]*) ([^ ]+) ([^ ]+) ([^ ]+) ([^ ]+) ([^ ]+) - (.*)$");

regexp!(MESSAGE_PREAMBLE => "^([^ ]+) ");

regexp!(MEASUREMENT_PATTERN => "^([^ ,]+)(?:,([^ ]+))? ");

regexp!(FIELD_VALUE_PATTERN => "^((t|T|(?:[Tt]rue)|TRUE)|(f|F|(?:[Ff]alse)|FALSE)|(?:(-?[0-9.]+)(i)?))(?:[ ,]|$)");

pub fn parse_prusa_metrics_packet(data: &[u8]) -> Result<Vec<InfluxDBPoint>> {
    let msg = SyslogMessage::parse(data)?;

    let lines = {
        let m = MESSAGE_PREAMBLE
            .exec(msg.message)
            .ok_or_else(|| err_msg("Failed to find message preamble"))?;
        &msg.message[m.last_index()..]
    };

    let mut out = vec![];
    for line in lines.lines() {
        out.push(InfluxDBPoint::parse_line(line)?);
    }

    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct InfluxDBPoint {
    pub measurement: String,
    pub tags: Vec<(String, String)>,
    pub fields: Vec<(String, InfluxDBValue)>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InfluxDBValue {
    Integer(i64),
    Float(f32),
    String(String),
    Bool(bool),
}

impl InfluxDBPoint {
    pub fn parse_line(data: &str) -> Result<Self> {
        let m = MEASUREMENT_PATTERN
            .exec(data)
            .ok_or_else(|| err_msg("Missing measurement name"))?;

        let measurement = m.group_str(1).unwrap()?;

        let mut tags = vec![];
        if let Some(tag_set) = m.group_str(2) {
            let tag_set = tag_set?;

            for pair in tag_set.split(',') {
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| err_msg("Missing tag pair delimiter"))?;
                tags.push((key.to_string(), value.to_string()));
            }
        }

        let mut rest = &data[m.last_index()..];

        let mut fields = vec![];
        while !rest.is_empty() {
            if rest.starts_with(" ") {
                break;
            }

            let (field_name, r) = rest
                .split_once('=')
                .ok_or_else(|| err_msg("Missing field name separator"))?;
            rest = r;

            let field_value = {
                if let Some(r) = rest.strip_prefix("\"") {
                    let mut s = String::new();

                    let mut chars = r.chars();

                    let mut escaped = false;
                    let mut done = false;
                    while let Some(c) = chars.next() {
                        if escaped {
                            s.push(c);
                            escaped = false;
                            continue;
                        }

                        if c == '"' {
                            done = true;
                            break;
                        }

                        if c == '\\' {
                            escaped = true;
                            continue;
                        }

                        s.push(c);
                    }

                    rest = chars.as_str();

                    if !done || escaped {
                        return Err(err_msg("Invalid string value"));
                    }

                    InfluxDBValue::String(s)
                } else {
                    let m = FIELD_VALUE_PATTERN
                        .exec(rest)
                        .ok_or_else(|| err_msg("Unknown field value format"))?;

                    rest = &rest[m.group(1).unwrap().len()..];

                    if m.group_str(2).is_some() {
                        InfluxDBValue::Bool(true)
                    } else if m.group_str(3).is_some() {
                        InfluxDBValue::Bool(false)
                    } else {
                        let is_int = m.group_str(5).is_some();

                        let num_str = m.group_str(4).ok_or_else(|| err_msg("Bad pattern"))??;

                        if is_int {
                            InfluxDBValue::Integer(num_str.parse()?)
                        } else {
                            InfluxDBValue::Float(num_str.parse()?)
                        }
                    }
                }
            };

            fields.push((field_name.to_string(), field_value));

            if let Some(r) = rest.strip_prefix(",") {
                rest = r;
            } else {
                break;
            }
        }

        rest = rest.trim();

        let timestamp = {
            if rest.is_empty() {
                None
            } else {
                Some(rest.parse()?)
            }
        };

        Ok(Self {
            measurement: measurement.to_string(),
            tags,
            fields,
            timestamp,
        })
    }
}

/// See https://tools.ietf.org/html/rfc5424
#[derive(Debug)]
pub struct SyslogMessage<'a> {
    priority: usize,
    version: usize,
    timestamp: &'a str,
    hostname: &'a str,
    app_name: &'a str,
    process_id: &'a str,
    message_id: &'a str,

    // NOTE: In the general case, this is allowed to be binary data.
    message: &'a str,
}

impl<'a> SyslogMessage<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let m = SYSLOG_HEADER_PATTERN
            .exec(data)
            .ok_or_else(|| err_msg("Failed to parse syslog header"))?;

        let priority = m.group_str(1).unwrap()?.parse()?;
        let version = m.group_str(2).unwrap()?.parse()?;
        let timestamp = m.group_str(3).unwrap()?;
        let hostname = m.group_str(4).unwrap()?;
        let app_name = m.group_str(5).unwrap()?;
        let process_id = m.group_str(6).unwrap()?;
        let message_id = m.group_str(7).unwrap()?;
        let message = m.group_str(8).unwrap()?;

        Ok(SyslogMessage {
            priority,
            version,
            timestamp,
            hostname,
            app_name,
            process_id,
            message_id,
            message,
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn works() -> Result<()> {
        let test_packets: &'static [&'static [u8]] = &[
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6275,tm=5333474297,v=4 fan,fan=enclosure state=0,pwm=0,measured=0 -411\neth_out sent=438257i -61\nheap free=48924i,total=76872i 46251\nbed_curr,n=0 v=0.052,e=0.000 166365\nbed_curr,n=1 v=0.046,e=0.000 166405\nadj_z v=0.000000 330195\nsplitter_5V_current v=0.214343 513069\n24VVoltage v=24.314516 513075\n5VVoltage v=5.048469 513080\nSandwitch5VCurrent v=0.470955 513089\nxlbuddy5VCurrent v=0.435035 513093\nheap free=48924i,total=76872i 550206\ncpu_usage v=39i 658152\nactive_extruder v=0i 658182\ndwarf_board_temp v=29i 910242\npoints_dropped v=0i 910315\nfan,fan=print state=0,pwm=0,measured=0 987114\nfan,fan=heatbreak state=0,pwm=0,measured=0 987143\nfan,fan=enclosure state=0,pwm=0,measured=0 987173\nheap free=48924i,total=76872i 1054176\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6276,tm=5334528714,v=4 eth_out sent=439065i -63\nbed_curr,n=0 v=0.051,e=0.000 165949\nbed_curr,n=1 v=0.044,e=0.000 165988\nsplitter_5V_current v=0.214343 471564\n24VVoltage v=24.314516 471575\n5VVoltage v=5.048469 471580\nSandwitch5VCurrent v=0.482928 471584\nxlbuddy5VCurrent v=0.443017 471588\nheap free=48924i,total=76872i 503787\ncpu_usage v=39i 604734\nactive_extruder v=0i 604763\nadj_z v=0.000000 776749\ndwarf_board_temp v=29i 857805\npoints_dropped v=0i 857911\nfan,fan=print state=0,pwm=0,measured=0 921301\nfan,fan=heatbreak state=0,pwm=0,measured=0 921331\nfan,fan=enclosure state=0,pwm=0,measured=0 921360\nheap free=48924i,total=76872i 1006821\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6277,tm=5335535777,v=4 eth_out sent=439789i -57\nbed_curr,n=0 v=0.051,e=0.000 200898\nbed_curr,n=1 v=0.044,e=0.000 200943\nsplitter_5V_current v=0.214343 477418\n24VVoltage v=24.278492 477424\n5VVoltage v=5.053926 477434\nSandwitch5VCurrent v=0.466964 477438\nxlbuddy5VCurrent v=0.447008 477443\nstack,n=default t=0,m=673 492651\nruntime,n=default u=10 492675\nstack,n=IDLE t=0,m=108 492693\nruntime,n=IDLE u=79 492715\nstack,n=network t=0,m=738 492733\nruntime,n=network u=0 492750\nstack,n=connect t=0,m=862 492768\nruntime,n=connect u=0 492790\nstack,n=measure t=0,m=178 492809\nruntime,n=measure u=0 492825\nstack,n=display t=0,m=1043 492844\nruntime,n=display u=1 492866\nstack,n=tcpip_t t=4992,m=910 492885\nruntime,n=tcpip_t u=0 492908\nstack,n=usb_dev t=0,m=497 492921\nruntime,n=usb_dev u=0 492943\nstack,n=puppies t=3584,m=253 492962\nruntime,n=puppies u=5 492985\nstack,n=acFault t=0,m=56 492998\nruntime,n=acFault u=0 493024\nstack,n=USBH_MS t=0,m=371 493042\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6278,tm=5336030070,v=4 runtime,n=USBH_MS u=0 -1229\nstack,n=USBH_Th t=1284,m=177 -1210\nruntime,n=USBH_Th u=0 -1192\nstack,n=metric_ t=0,m=120 -1174\nruntime,n=metric_ u=0 -1152\nstack,n=TmrSvc t=0,m=64 -1133\nruntime,n=TmrSvc u=1 -1116\nstack,n=media_p t=0,m=844 -1098\nruntime,n=media_p u=0 -1076\nstack,n=esp_tas t=0,m=91 -1058\nruntime,n=esp_tas u=0 -1041\nheap free=48924i,total=76872i 9421\ncpu_usage v=39i 104379\nactive_extruder v=0i 104409\ndwarf_board_temp v=29i 358441\npoints_dropped v=0i 358500\nfan,fan=print state=0,pwm=0,measured=0 408549\nfan,fan=heatbreak state=0,pwm=0,measured=0 408580\nfan,fan=enclosure state=0,pwm=0,measured=0 408609\nheap free=48924i,total=76872i 513419\nbed_curr,n=0 v=0.050,e=0.000 746592\nbed_curr,n=1 v=0.043,e=0.000 746632\nadj_z v=0.000000 775422\nsplitter_5V_current v=0.214343 996046\n24VVoltage v=24.314516 996052\n5VVoltage v=5.048469 996057\nSandwitch5VCurrent v=0.470955 996066\nxlbuddy5VCurrent v=0.443017 996070\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6279,tm=5337026762,v=4 eth_out sent=441838i -123\nheap free=48924i,total=76872i 20714\ncpu_usage v=39i 108688\nactive_extruder v=0i 108723\ndwarf_board_temp v=29i 363750\npoints_dropped v=0i 363809\nfan,fan=print state=0,pwm=0,measured=0 399453\nfan,fan=heatbreak state=0,pwm=0,measured=0 399485\nfan,fan=enclosure state=0,pwm=0,measured=0 399514\nheap free=48924i,total=76872i 523739\nbed_curr,n=0 v=0.052,e=0.000 778907\nbed_curr,n=1 v=0.043,e=0.000 778951\nprint_filename v=\"\" 987786\nis_printing v=0i 998713\nsplitter_5V_current v=0.214343 1012266\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6280,tm=5338039269,v=4 24VVoltage v=24.314516 -226\n5VVoltage v=5.048469 -221\nSandwitch5VCurrent v=0.462973 -217\nxlbuddy5VCurrent v=0.435035 -208\neth_out sent=442459i -67\nheap free=48924i,total=76872i 15226\ncpu_usage v=39i 97185\nactive_extruder v=0i 97219\nadj_z v=0.000000 266242\ndwarf_board_temp v=29i 353245\npoints_dropped v=0i 353321\nfan,fan=print state=0,pwm=0,measured=0 375551\nfan,fan=heatbreak state=0,pwm=0,measured=0 375581\nfan,fan=enclosure state=0,pwm=0,measured=0 375610\nheap free=48924i,total=76872i 519235\nbed_curr,n=0 v=0.052,e=0.000 770392\nbed_curr,n=1 v=0.043,e=0.000 770432\nstack,n=default t=0,m=673 990018\nruntime,n=default u=10 990042\nstack,n=IDLE t=0,m=108 990056\nruntime,n=IDLE u=79 990077\nstack,n=puppies t=3584,m=253 990097\nruntime,n=puppies u=5 990120\nstack,n=network t=0,m=738 990134\nruntime,n=network u=0 990160\nstack,n=connect t=0,m=862 990178\nruntime,n=connect u=0 990201\nstack,n=display t=0,m=1043 990219\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6281,tm=5339030628,v=4 runtime,n=display u=1 -1118\nstack,n=tcpip_t t=4992,m=910 -1105\nruntime,n=tcpip_t u=0 -1079\nstack,n=usb_dev t=0,m=497 -1060\nruntime,n=usb_dev u=0 -1039\nstack,n=measure t=0,m=178 -1025\nruntime,n=measure u=0 -992\nstack,n=USBH_MS t=0,m=371 -979\nruntime,n=USBH_MS u=0 -939\nstack,n=USBH_Th t=1284,m=177 -926\nruntime,n=USBH_Th u=0 -903\nstack,n=media_p t=0,m=844 -884\nruntime,n=media_p u=0 -862\nstack,n=metric_ t=0,m=120 -849\nruntime,n=metric_ u=0 -823\nstack,n=TmrSvc t=0,m=64 -805\nruntime,n=TmrSvc u=1 -783\nstack,n=esp_tas t=0,m=91 -765\nruntime,n=esp_tas u=0 -748\nstack,n=acFault t=0,m=56 -730\nruntime,n=acFault u=0 -708\nsplitter_5V_current v=0.214343 21317\n24VVoltage v=24.314516 21323\n5VVoltage v=5.048469 21333\nSandwitch5VCurrent v=0.462973 21337\nxlbuddy5VCurrent v=0.443017 21342\nheap free=48924i,total=76872i 31870\ncpu_usage v=39i 106820\nactive_extruder v=0i 106850\ndwarf_board_temp v=29i 363886\npoints_dropped v=0i 363945\n",
            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=6282,tm=5339394779,v=4 eth_out sent=444503i -58\nfan,fan=print state=0,pwm=0,measured=0 7636\nfan,fan=heatbreak state=0,pwm=0,measured=0 7667\nfan,fan=enclosure state=0,pwm=0,measured=0 7696\nheap free=48924i,total=76872i 170744\nfilament v=\"PETG\" 286697\nadj_z v=0.000000 411689\nbed_curr,n=0 v=0.051,e=0.000 439884\nbed_curr,n=1 v=0.044,e=0.000 439923\nsplitter_5V_current v=0.214343 670087\n24VVoltage v=24.278492 670093\n5VVoltage v=5.048469 670098\nSandwitch5VCurrent v=0.490911 670107\nxlbuddy5VCurrent v=0.439026 670111\nheap free=48924i,total=76872i 674698\ncpu_usage v=39i 743673\nactive_extruder v=0i 743707\nfan,fan=print state=0,pwm=0,measured=0 995233\nfan,fan=heatbreak state=0,pwm=0,measured=0 995263\nfan,fan=enclosure state=0,pwm=0,measured=0 995292\ndwarf_board_temp v=29i 1001738\n",

            // These use negative numbers in the values

            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=126471,tm=103766389199,v=4 eth_out sent=107977508i -65\ndwarf_board_temp v=31i 162320\npoints_dropped v=0i 162383\nheap free=48972i,total=76872i 302299\nadj_z v=0.000000 303302\ncpu_usage v=40i 460256\nbed_curr,n=0 v=0.064,e=0.000 533480\nbed_curr,n=1 v=0.059,e=0.000 533524\nactive_extruder v=1i 619270\nloadcell_scale v=0.019200 626379\nloadcell_threshold v=-125.000000 626386\nloadcell_threshold_cont v=-40.000000 626395\nloadcell_hysteresis v=80.000000 626400\nheap free=48972i,total=76872i 806311\nprint_filename v=\"\" 963319\nsplitter_5V_current v=0.214343 980119\n24VVoltage v=24.314516 980126\n5VVoltage v=5.048469 980130\nSandwitch5VCurrent v=0.510866 980134\nxlbuddy5VCurrent v=0.439026 980138\nfan,fan=print state=0,pwm=0,measured=0 987205\nfan,fan=heatbreak state=0,pwm=0,measured=0 987237\nfan,fan=enclosure state=0,pwm=0,measured=0 987270\nis_printing v=0i 1011251\n",

            b"<14>1 - 10:9c:70:20:8d:3 buddy - - - msg=127112,tm=104286995867,v=4 24VVoltage v=24.314516 -292\n5VVoltage v=5.053926 -288\nSandwitch5VCurrent v=0.502884 -284\nxlbuddy5VCurrent v=0.443017 -280\neth_out sent=108551813i -120\nloadcell_scale v=0.019200 47714\nloadcell_threshold v=-125.000000 47725\nloadcell_threshold_cont v=-40.000000 47730\nloadcell_hysteresis v=80.000000 47734\nheap free=48972i,total=76872i 99679\nfan,fan=print state=0,pwm=0,measured=0 113142\nfan,fan=heatbreak state=0,pwm=0,measured=0 113181\nfan,fan=enclosure state=0,pwm=0,measured=0 113210\nprint_filename v=\"\" 489654\nis_printing v=0i 535630\ncpu_usage v=40i 570616\nheap free=48972i,total=76872i 603666\ndwarf_board_temp v=31i 624617\npoints_dropped v=0i 624694\nbed_curr,n=0 v=0.064,e=0.000 726792\nbed_curr,n=1 v=0.059,e=0.000 726836\nactive_extruder v=1i 767613\nadj_z v=0.000000 1002632\n"
        ];

        /*
        [10.100.0.2:55034] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=11376,tm=8972691150,v=4 runtime,n=TmrSvc u=2 -2041\nstack,n=esp_tas t=0,m=99 -2020\nruntime,n=esp_tas u=0 -1993\nstack,n=media_p t=0,m=358 -1972\nruntime,n=media_p u=0 -1949\nadj_z v=0.000000 42299\nbed_curr,n=0 v=2.876,e=2.687 82508\nbed_curr,n=1 v=1.997,e=1.804 82564\nfan,fan=print state=3,pwm=47,measured=2276 97877\nfan,fan=heatbreak state=3,pwm=100,measured=10251 97910\nfan,fan=enclosure state=3,pwm=49,measured=1285 97949\nmedia_prefetched v=5683i 105512\ncpu_usage v=55i 105553\nactive_extruder v=0i 138301\ndwarf_board_temp v=55i 202355\npoints_dropped v=3504i 202428\nheap free=45696i,total=76872i 220346\nxlbuddy5VCurrent v=0.462973 243747\nis_printing v=1i 470277\nprint_filename v="3DBenchy_0.4n_0.2mm_PETG_XLIS_48m.gcode" 514849\n24VVoltage v=24.242474 531405\n5VVoltage v=5.048469 615478\nsplitter_5V_current v=0.214343 696509\nheap free=45696i,total=76872i 725347\nSandwitch5VCurrent v=0.690468 939613\nloadcell_scale v=0.019200 975382\n



        [10.100.0.2:60466] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=1174,tm=1170073585,v=4 tmc_read,ax=? reg=111i,regn="drv_status",value=1442005i -34\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442001i 38989\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442001i 77871\nactive_extruder v=0i 91857\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441993i 116990\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442010i 155971\npoints_dropped v=0i 173908\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442023i 194983\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442000i 233969\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441997i 273123\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442001i 311973\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442006i 350943\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442009i 389971\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442004i 428956\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442001i 468129\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442017i 506944\n
        [10.100.0.2:60466] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=1175,tm=1170619559,v=4 tmc_read,ax=? reg=111i,regn="drv_status",value=1442007i -30\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442013i 38983\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442002i 77968\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442001i 116966\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441993i 155995\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441997i 195016\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442011i 233994\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441986i 273030\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441986i 312149\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441986i 351027\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442015i 389993\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442020i 428995\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441991i 467995\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442000i 507141\nactive_extruder v=0i 545907\n
        [10.100.0.2:60466] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=1176,tm=1171165794,v=4 tmc_read,ax=? reg=111i,regn="drv_status",value=1441990i -35\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442006i 38874\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441994i 77733\npoints_dropped v=0i 83680\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441984i 116760\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442002i 155947\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442023i 194734\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442018i 233769\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442020i 272775\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441996i 311736\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441996i 350914\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441995i 389745\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442012i 428761\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441996i 467748\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441993i 506736\n
        [10.100.0.2:60466] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=1177,tm=1171712270,v=4 tmc_read,ax=? reg=111i,regn="drv_status",value=1441987i -527\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442014i 38266\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442022i 77332\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442012i 116268\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442022i 156282\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441999i 195285\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442000i 234442\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442009i 273257\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442001i 312298\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442009i 351259\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441998i 390285\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442000i 429198\nactive_extruder v=0i 454170\ntmc_read,ax=? reg=111i,regn="drv_status",value=1441978i 468311\ntmc_read,ax=? reg=111i,regn="drv_status",value=1442014i 507283\n


        [10.100.0.2:60466] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=1216,tm=1224206569,v=4 active_extruder v=0i -92\nmedia_prefetched v=5508i 143101\ncpu_usage v=49i 143133\nloadcell_scale v=0.019200 143252\nloadcell_threshold v=-125.000000 143256\nloadcell_threshold_cont v=-40.000000 143260\nloadcell_hysteresis v=80.000000 143270\npoints_dropped v=0i 160932\nactive_extruder v=0i 1000970\n


        [10.100.0.2:53971] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=1037,tm=281335829,v=4 pos_y v=-9.000000 -294\npos_x v=-8.000000 11747\npos_y v=-9.000000 11754\npos_x v=-8.000000 23686\npos_y v=-9.000000 23697\npos_x v=-8.000000 35688\npos_y v=-9.000000 35699\npos_x v=-8.000000 47687\npos_y v=-9.000000 47702\npos_x v=-8.000000 59714\npos_y v=-9.000000 59725\npos_x v=-8.000000 71657\npos_y v=-9.000000 71664\npos_x v=-8.000000 82726\npos_y v=-9.000000 82732\ngcode v="M332 pos_x" 83811\nloadcell_scale v=0.019200 83827\nloadcell_threshold v=-125.000000 83831\nloadcell_threshold_cont v=-40.000000 83835\nloadcell_hysteresis v=80.000000 83838\npos_y v=-9.000000 94658\npos_y v=-9.000000 105688\npos_y v=-9.000000 117687\npos_y v=-9.000000 128705\nsdpos v=-1i 135666\npos_y v=-9.000000 140684\npos_y v=-9.000000 152685\npos_y v=-9.000000 164660\npos_y v=-9.000000 175688\npos_y v=-9.000000 187685\npos_y v=-9.000000 199687\npos_y v=-9.000000 210709\npos_y v=-9.000000 222676\npos_y v=-9.000000 234687\npos_y v=-9.000000 246685\n


        # This is the best example of the typical line we see with the current settings.
        [10.100.0.2:53976] <14>1 - 10:9c:70:20:8d:3 buddy - - - msg=276396,tm=44701604817,v=4 pos_y v=290.312500 -260\npos_z v=17.433750 -249\npos_x v=94.587502 10790\npos_y v=290.312500 10802\npos_z v=17.433750 10806\npos_x v=94.587502 22686\npos_y v=290.312500 22692\npos_z v=17.433750 22705\npos_x v=94.587502 33718\npos_y v=290.312500 33729\npos_z v=17.433750 33733\npos_x v=94.587502 45689\npos_y v=290.312500 45701\npos_z v=17.433750 45705\npos_x v=94.587502 56734\npos_y v=290.312500 56740\npos_z v=17.433750 56751\npos_x v=94.587502 68750\npos_y v=290.312500 68757\npos_z v=17.433750 68761\nactive_extruder v=0i 79727\npos_x v=94.587502 80674\npos_y v=290.312500 80680\npos_z v=17.433750 80685\npos_x v=94.587502 91740\npos_y v=290.312500 91754\npos_z v=17.433750 91758\npos_x v=94.587502 103689\npos_y v=290.312500 103701\npos_z v=17.433750 103705\npos_x v=94.587502 114723\npos_y v=290.312500 114730\npos_z v=17.433750 114734\npos_x v=94.587502 126703\npos_y v=290.312500 126709\npos_z v=17.433750 126714\npos_x v=94.587502 138750\n

         */

        for packet in test_packets {
            println!("{:?}", parse_prusa_metrics_packet(*packet)?);
        }

        Ok(())
    }

    // Bunch of test cases from:
    // - https://docs.influxdata.com/influxdb/v1/write_protocols/line_protocol_reference/
    // - https://docs.influxdata.com/influxdb/v1/write_protocols/line_protocol_tutorial/
    #[test]
    fn influx_db_parsing() -> Result<()> {
        let test_cases: Vec<(&'static str, InfluxDBPoint)> = vec![
            (
                "weather,location=us-midwest,season=summer temperature=82 1465839830100400200",
                InfluxDBPoint {
                    measurement: "weather".into(),
                    tags: vec![
                        ("location".into(), "us-midwest".into()),
                        ("season".into(), "summer".into()),
                    ],
                    fields: vec![("temperature".into(), InfluxDBValue::Float(82.0))],
                    timestamp: Some(1465839830100400200),
                },
            ),
            (
                "measurementName fieldKey=\"field string value\" 1556813561098000000",
                InfluxDBPoint {
                    measurement: "measurementName".into(),
                    tags: vec![],
                    fields: vec![(
                        "fieldKey".into(),
                        InfluxDBValue::String("field string value".into()),
                    )],
                    timestamp: Some(1556813561098000000),
                },
            ),
            (
                "weather temperature=82 1465839830100400200",
                InfluxDBPoint {
                    measurement: "weather".into(),
                    tags: vec![],
                    fields: vec![("temperature".into(), InfluxDBValue::Float(82.0))],
                    timestamp: Some(1465839830100400200),
                },
            ),
            (
                "weather,location=us-midwest temperature=82,humidity=71 1465839830100400200",
                InfluxDBPoint {
                    measurement: "weather".into(),
                    tags: vec![("location".into(), "us-midwest".into())],
                    fields: vec![
                        ("temperature".into(), InfluxDBValue::Float(82.0)),
                        ("humidity".into(), InfluxDBValue::Float(71.0)),
                    ],
                    timestamp: Some(1465839830100400200),
                },
            ),
            (
                "weather,location=us-midwest temperature=82",
                InfluxDBPoint {
                    measurement: "weather".into(),
                    tags: vec![("location".into(), "us-midwest".into())],
                    fields: vec![("temperature".into(), InfluxDBValue::Float(82.0))],
                    timestamp: None,
                },
            ),
        ];

        for (input, point) in test_cases {
            assert_eq!(InfluxDBPoint::parse_line(input)?, point);
        }

        Ok(())
    }
    /*
    TODO: Add these:

    weather,location=us-midwest temperature=82 1465839830100400200

    weather,location=us-midwest temperature=82i 1465839830100400200

    weather,location=us-midwest temperature="too warm" 1465839830100400200

    weather,location=us-midwest too_hot=true 1465839830100400200

    weather,location=us\,midwest temperature=82 1465839830100400200

    weather,location=us-midwest temp\=rature=82 1465839830100400200

    weather,location\ place=us-midwest temperature=82 1465839830100400200

    wea\,ther,location=us-midwest temperature=82 1465839830100400200

    wea\ ther,location=us-midwest temperature=82 1465839830100400200

    weather,location=us-midwest temperature="too\"hot\"" 1465839830100400200

    */
}
