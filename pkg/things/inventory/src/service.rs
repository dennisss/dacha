use std::sync::Arc;

use common::errors::*;
use cluster_client::ClusterMetaClient;
use inventory_proto::inventory::*;
use db_table::*;
use crypto::random::RngExt;
use cluster_client::service::create_rpc_channel;
use labeler_proto::labeler;

use crate::tables::*;

pub struct InventoryImpl {
    client: Arc<ClusterMetaClient>
}

impl InventoryImpl {
    pub async fn create(client: Arc<ClusterMetaClient>) -> Result<Self> {
        Ok(Self { client })
    }

    async fn query_entities_impl(&self, request: &QueryEntitiesRequest) -> Result<QueryEntitiesResponse> {
        let txn = self.client.db().new_transaction().await?;
        let parts = txn.list::<PartTable>().await?;
        let packs = txn.list::<PackTable>().await?;

        let mut out = QueryEntitiesResponse::default();
        for part in parts {
            out.add_parts(part);
        }
        for pack in packs {
            out.add_packs(pack);
        }
        Ok(out)
    }

    async fn update_entities_impl(&self, request: &UpdateEntitiesRequest) -> Result<UpdateEntitiesResponse> {
        let mut out = UpdateEntitiesResponse::default();

        let mut txn = self.client.db().new_transaction().await?;
        
        for part in request.update_parts() {
            let mut part = part.as_ref().clone();
            if part.id() == 0 {
                part.set_id(crypto::random::clocked_rng().uniform::<u64>());
            }
            txn.put::<PartTable>(&part).await?;

            out.add_parts(part);
        }

        for part_id in request.delete_parts() {
            // TODO: Check no references
            // let pack_refs = query!()
        }

        for pack in request.update_packs() {
            let mut pack = pack.as_ref().clone();
            if pack.id() == 0 {
                pack.set_id(crypto::random::clocked_rng().uniform::<u64>());
            }

            if pack.part_id() == 0 {
                return Err(rpc::Status::invalid_argument("Missing part_id for pack").into());
            }

            txn.put::<PackTable>(&pack).await?;

            out.add_packs(pack);
        }
        
        for pack_id in request.delete_packs() {
            // TODO: check no references
        }

        txn.commit().await?;

        Ok(out)
    }

    async fn print_labels_impl(&self, request: &PrintLabelsRequest) -> Result<PrintLabelsResponse> {
        let mut out = PrintLabelsResponse::default();

        // TODO: Cache this.
        let channel = create_rpc_channel(
            "labeler.job.local.cluster.internal",
            self.client.clone()
        ).await?;

        let stub = labeler::LabelerStub::new(channel);
        let ctx = rpc::ClientRequestContext::default();

        // TODO: Eventually may need multiple devices if printing at different widths.

        // Find the device with 12mm width
        let device = {
            let req = labeler::ListDevicesRequest::default();
            let res = stub.ListDevices(&ctx, &req).await.result?;

            let mut found_dev = None;
            for dev in res.devices() {
                if dev.tape().width() == 85 && dev.tape().dpi() == 180 { // 12mm
                    found_dev = Some(dev.as_ref().clone());
                    break;
                }
            }

            found_dev.ok_or_else(|| rpc::Status::not_found("No device found with right tape width"))?
        };

        // Generate the print requests requests.
        let mut inner_req = labeler::PrintLabelRequest::default();
        inner_req.set_device_id(device.id());
        inner_req.set_dry_run(request.dry_run());

        let txn = self.client.db().new_transaction().await?;

        for pack_id in request.pack_ids() {
            let pack = query_one!(txn, PackTable, "id = ?", *pack_id)
                .ok_or_else(|| rpc::Status::not_found("Missing pack"))?;
            
            let part = query_one!(txn, PartTable, "id = ?", pack.part_id())
                .ok_or_else(|| rpc::Status::not_found("Missing part"))?;

            let page = inner_req.new_pages();
            page.set_max_length_mm(46.0);
            page.set_length_mm(46.0);
            page.set_quantity(1u32);
            page.text_mut().set_value(part.name());
            page.text_mut().set_align(labeler::LabelText_TextAlignment::LEFT);
            
            let dm_data = format!("da:pack:{}", base_radix::base64url_encode(&pack.id().to_be_bytes()));
            page.datamatrix_mut().set_data(dm_data);
            page.datamatrix_mut().set_position(labeler::LabelDatamatrix_Position::LEFT_OF_TEXT);
        }

        let inner_res = stub.Print(&ctx, &inner_req).await.result?;

        for (i, pack_id) in request.pack_ids().iter().enumerate() {
            let mut r = out.new_labels();
            r.set_device(device.clone());
            // r.set_device(inner_res.device().clone()); // TODO: Fix this
            r.add_page_images(inner_res.page_images()[i].as_ref().clone());
        }

        Ok(out)
    }
}

#[async_trait]
impl InventoryService for InventoryImpl {
    async fn QueryEntities(
        &self,
        request: rpc::ServerRequest<QueryEntitiesRequest>,
        response: &mut rpc::ServerResponse<QueryEntitiesResponse>,
    ) -> Result<()> {
        response.value = self.query_entities_impl(&request.value).await?;
        Ok(())
    }

    async fn UpdateEntities(
        &self,
        request: rpc::ServerRequest<UpdateEntitiesRequest>,
        response: &mut rpc::ServerResponse<UpdateEntitiesResponse>,
    ) -> Result<()> {
        response.value = self.update_entities_impl(&request.value).await?;
        Ok(())
    }

    async fn PrintLabels(
        &self,
        request: rpc::ServerRequest<PrintLabelsRequest>,
        response: &mut rpc::ServerResponse<PrintLabelsResponse>,
    ) -> Result<()> {
        response.value = self.print_labels_impl(&request.value).await?;
        Ok(())
    }
}