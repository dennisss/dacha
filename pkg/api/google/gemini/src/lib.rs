#[macro_use]
extern crate macros;

use std::convert::TryFrom;
use std::sync::Arc;

use base_error::*;
use google_auth::GoogleServiceAccount;
use http::uri::Uri;
use protobuf::{Enum, EnumReflection};
use googleapis_proto::google::ai::generativelanguage::v1::*;

const PRODUCTION_TARGET: &'static str = "https://generativelanguage.googleapis.com";


pub struct GeminiClient {
    stub: GenerativeServiceStub
}

impl GeminiClient {

    pub async fn create(service_account: Arc<GoogleServiceAccount>) -> Result<Self> {
        let service_uri: Uri = Uri::try_from(PRODUCTION_TARGET)?;
    
        let creds = google_auth::GoogleServiceAccountJwtCredentials::create(
            service_uri.clone(),
            service_account.clone(),
        )?;
    
        let mut channel_options =
            rpc::Http2ChannelOptions::try_from(http::ClientOptions::from_uri(&service_uri)?)?;
        channel_options.credentials = Some(Box::new(creds));
    
        let channel = Arc::new(rpc::Http2Channel::create(channel_options).await?);
    
        let stub = GenerativeServiceStub::new(channel.clone());
        Ok(Self {
            stub
        })
    }

    pub async fn generate_text(&self, prompt: &str, image: Option<&[u8]>) -> Result<String> {

        let mut request = GenerateContentRequest::default();
    
        request.generation_config_mut().set_max_output_tokens(8192);
        request.generation_config_mut().set_top_p(0.95);
        request.generation_config_mut().set_top_k(40);
        request.generation_config_mut().set_temperature(1.0);
    
        let supported_categories = [
            HarmCategory::HARM_CATEGORY_HATE_SPEECH,
            HarmCategory::HARM_CATEGORY_SEXUALLY_EXPLICIT,
            HarmCategory::HARM_CATEGORY_DANGEROUS_CONTENT,
            HarmCategory::HARM_CATEGORY_HARASSMENT,
            HarmCategory::HARM_CATEGORY_CIVIC_INTEGRITY,
        ];
    
        // Disable all safety filters.
        for cat in supported_categories {
            let setting = request.new_safety_settings();
            setting.set_category(cat);
            setting.set_threshold(SafetySetting_HarmBlockThreshold::OFF);
        }
    
        request.set_model("models/gemini-2.0-flash");
    
        let chunk = request.new_contents();
        chunk.set_role("user");

        if let Some(image) = image {
            let part = chunk.new_parts();
            part.inline_data_mut().set_mime_type("image/jpeg");
            part.inline_data_mut().set_data(image);
        }
    
        {
            let part = chunk.new_parts();
            part.set_text(prompt);
        }
    
        let response = self.stub
            .GenerateContent(&rpc::ClientRequestContext::default(), &request)
            .await
            .result?;
    
        if response.candidates().len() != 1 || response.candidates()[0].content().parts().len() != 1 {
            return Err(err_msg("Received malformed gemini response"));
        }

        Ok(response.candidates()[0].content().parts()[0].text().to_string())
    }
}