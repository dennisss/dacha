#[macro_export]
macro_rules! impl_resource_passthrough {
    ($t:ident, $field:ident) => {
        #[async_trait]
        impl $crate::ServiceResource for $t {
            async fn add_cancellation_token(
                &self,
                token: ::std::sync::Arc<dyn executor::cancellation::CancellationToken>,
            ) {
                self.$field.add_cancellation_token(token).await
            }

            async fn new_resource_subscriber(&self) -> Box<dyn $crate::ServiceResourceSubscriber> {
                self.$field.new_resource_subscriber().await
            }
        }
    };
}
