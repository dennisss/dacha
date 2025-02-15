use std::{sync::Arc, time::Duration};

use base_error::*;
use screen_grabber_proto::screen_grabber::*;
use image::format::jpeg::encoder::JPEGEncoder;
use image::{Color, Image};
use gemini::GeminiClient;
use google_auth::GoogleServiceAccount;

pub struct ScreenGrabberImpl {
    gemini: GeminiClient
}

impl ScreenGrabberImpl {
    pub async fn create(service_account: Arc<GoogleServiceAccount>) -> Result<Self> {
        Ok(Self { gemini: GeminiClient::create(service_account).await? })
    }

    async fn list_windows_impl(&self, request: &ListWindowsRequest) -> Result<ListWindowsResponse> {
        let mut res = ListWindowsResponse::default();

        let display = x11::Display::open_default()?;
        let root_window = display.root_window()?;
        let sub_windows = root_window.client_list()?;

        // TODO: Also add the root window.

        for window in sub_windows {
            let proto = res.new_windows();
            proto.set_id(window.id());
            proto.set_name(window.name()?.unwrap_or_else(|| format!("Unknown")));
        }

        Ok(res)
    }

    async fn grab_impl(&self, request: &GrabRequest) -> Result<GrabResponse> {

        let mut res = GrabResponse::default();

        let image = self.grab_image(request.window_id())?;
        res.set_image(&image[..]);

        let text = self.gemini.generate_text("This is a screenshot of a video player with dialogue subtitles and user controls. Your task is to print the dialogue as a text string (excluding any text in UI controls). Do not print anything other than the dialogue text string and line breaks as a response.", Some(&image)).await?;
        res.set_text(text);

        /*
                
        println!("{:?}", attrs);
    
        let start = Instant::now();
    
        
    
        let end = Instant::now();
    
        println!("Capture takes: {:?}", end - start);
    
        println!("{:?}", ximage);
    
        println!("LSB FIRST: {}", x11::bindings::LSBFirst);
    
        /*
        Data should be 32-bit
        */        
        */

        Ok(res)
    }

    fn grab_image(&self, window_id: u64) -> Result<Vec<u8>> {

        let display = x11::Display::open_default()?;
        let root_window = display.root_window()?;
        let sub_windows = root_window.client_list()?;

        let mut selected_window = None;

        for window in sub_windows {
            if window.id() == window_id {
                selected_window = Some(window);
                break;
            }
        }

        let selected_window = selected_window.ok_or_else(|| rpc::Status::not_found("No such window"))?;

        let attrs = selected_window.attrs()?;
        let ximage = selected_window.get_full_image(&attrs)?;
    
        // TODO: Check aligned.
    
        let data = unsafe {
            core::slice::from_raw_parts(
                core::mem::transmute::<_, *const u32>(ximage.data),
                (ximage.width * ximage.height) as usize,
            )
        };
    
        let mut out = Image::<u8>::zero(
            ximage.height as usize,
            ximage.width as usize,
            image::Colorspace::RGB,
        );
    
        for y in 0..out.height() {
            for x in 0..out.width() {
                let i = y * out.width() + x;
    
                let v = data[i];
    
                let r = ((v & (ximage.red_mask as u32)) >> 16) as u8;
                let g = ((v & (ximage.green_mask as u32)) >> 8) as u8;
                let b = ((v & (ximage.blue_mask as u32)) >> 0) as u8;
    
                out.set(y, x, &Color::rgb(r, g, b));
            }
        }

        let encoder = JPEGEncoder::new(80);
        let mut data = vec![];
        encoder.encode(&out, &mut data)?;

        Ok(data)
    }

}

#[async_trait]
impl ScreenGrabberService for ScreenGrabberImpl {
    async fn ListWindows(
        &self,
        request: rpc::ServerRequest<ListWindowsRequest>,
        response: &mut rpc::ServerResponse<ListWindowsResponse>,
    ) -> Result<()> {
        response.value = self.list_windows_impl(&request.value).await?;
        Ok(())
    }

    async fn Grab(
        &self,
        request: rpc::ServerRequest<GrabRequest>,
        response: &mut rpc::ServerResponse<GrabResponse>,
    ) -> Result<()> {
        response.value = self.grab_impl(&request.value).await?;
        Ok(())
    }
}
