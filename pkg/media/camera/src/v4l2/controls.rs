use std::collections::HashMap;

use common::errors::*;
use v4l2::{Controllable, ControlDefinition};
use media_camera_proto::media::camera::*;


pub async fn controls_to_proto(
    controls: &[ControlDefinition],
    device: &dyn Controllable,
    id_prefix: &str,
    group_prop: &mut Property
) -> Result<()> {
    let mut current_group = &mut *group_prop;

    for control in controls {
        let id = format!("{}{}", id_prefix, control.id());

        match control.typ() {
            v4l2::ControlType::CLASS => {
                drop(current_group);

                let mut prop = group_prop.new_children();
                prop.set_id(id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::GROUP);

                current_group = prop;
            }
            v4l2::ControlType::INTEGER => {
                let mut prop = current_group.new_children();
                prop.set_id(id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::INT32);
                prop.spec_mut()
                    .min_value_mut()
                    .set_int32_value(control.minimum());
                prop.spec_mut()
                    .max_value_mut()
                    .set_int32_value(control.maximum());
                prop.spec_mut()
                    .default_value_mut()
                    .set_int32_value(control.default_value());
                prop.spec_mut().step_mut().set_int32_value(control.step());

                prop.current_value_mut().set_int32_value(device.get_control_value(&control).await?);
            }
            v4l2::ControlType::BOOLEAN => {
                let mut prop = current_group.new_children();
                prop.set_id(id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::BOOL);

                // TODO: Convert to a bool
                prop.current_value_mut().set_int32_value(device.get_control_value(&control).await?);
            }
            v4l2::ControlType::MENU | v4l2::ControlType::INTEGER_MENU => {
                let mut prop = current_group.new_children();
                prop.set_id(id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::ENUM);

                for item in control.menu_items() {
                    let mut v = prop.spec_mut().new_values();
                    v.set_value_name(item.name()?);
                    v.set_int32_value(item.index() as i32);
                }

                prop.current_value_mut().set_int32_value(device.get_control_value(&control).await?);
            }
            v4l2::ControlType::Unknown(_) => {
                let mut prop = current_group.new_children();
                prop.set_id(id);
                prop.spec_mut().set_name(control.name()?);
            }
        }
    }

    Ok(())
}

/*
///
///
/// TODO: Perform validation that all controls are in a valid?
///
/// NOTE: Only controls listed in the 'controls' list will be allowed for modification.
pub async fn set_controls_from_proto(
    controls: &[ControlDefinition],
    device: &dyn Controllable,
    id_prefix: &str,
    old_proto: &mut Property,
    group_prop: &Property
) ->  Result<()> {
    // TODO: Use the id_prefix

    let mut controls_map = HashMap::default();
    for control in controls {
        controls_map.insert(control.id(), control);
    }




}
*/

