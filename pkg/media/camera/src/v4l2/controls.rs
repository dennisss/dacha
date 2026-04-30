use std::collections::HashMap;

use common::errors::*;
use v4l2::{Controllable, ControlDefinition};
use media_camera_proto::media::camera::*;


pub async fn controls_to_proto(
    controls: &[ControlDefinition],
    device: &dyn Controllable,
    id_prefix: &str,
    group_prop: &mut Property,
    state: &mut PropertiesState,
) -> Result<()> {
    let mut current_group = &mut *group_prop;

    for control in controls {
        let id = format!("{}{}", id_prefix, control.id());

        match control.typ() {
            v4l2::ControlType::CLASS => {
                drop(current_group);

                let mut prop = group_prop.new_children();
                prop.set_id(&id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::GROUP);

                current_group = prop;
            }
            v4l2::ControlType::INTEGER => {
                let mut prop = current_group.new_children();
                prop.set_id(&id);
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

                let state = state.new_states();
                state.set_id(id);
                state.current_value_mut().set_int32_value(device.get_control_value(&control).await?);
            }
            v4l2::ControlType::BOOLEAN => {
                let mut prop = current_group.new_children();
                prop.set_id(&id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::BOOL);

                // TODO: Convert to a bool
                let state = state.new_states();
                state.set_id(id);
                state.current_value_mut().set_int32_value(device.get_control_value(&control).await?);
            }
            v4l2::ControlType::MENU | v4l2::ControlType::INTEGER_MENU => {
                let mut prop = current_group.new_children();
                prop.set_id(&id);
                prop.spec_mut().set_name(control.name()?);
                prop.spec_mut().set_typ(PropertySpec_Type::ENUM);

                for item in control.menu_items() {
                    let mut v = prop.spec_mut().new_values();
                    v.set_value_name(item.name()?);
                    v.set_int32_value(item.index() as i32);
                }

                let state = state.new_states();
                state.set_id(id);
                state.current_value_mut().set_int32_value(device.get_control_value(&control).await?);
            }
            v4l2::ControlType::Unknown(_) => {
                let mut prop = current_group.new_children();
                prop.set_id(&id);
                prop.spec_mut().set_name(control.name()?);
            }
        }
    }

    Ok(())
}


/// NOTE: Only controls listed in the 'controls' list will be allowed for modification.
///
/// Returns a new PropertiesState object which is the same as
/// old_states but with changed controls modified.
pub async fn set_controls_from_proto(
    device: &dyn Controllable,
    controls: &[ControlDefinition],
    id_prefix: &str,
    old_states: &PropertiesState,
    new_states: &PropertiesState
) -> Result<PropertiesState> {
    let mut new_states_map = HashMap::<u32, &PropertyState>::default();
    for s in new_states.states() {
        let id = match s.id().strip_prefix(id_prefix) {
            Some(v) => v.parse::<u32>()?,
            None => continue
        };

        new_states_map.insert(id, s);
    }

    let mut out = PropertiesState::default();

    let mut old_states_map = HashMap::<u32, &PropertyState>::default();
    for s in old_states.states() {
        let id = match s.id().strip_prefix(id_prefix) {
            Some(v) => v.parse::<u32>()?,
            None => {
                out.add_states(s.as_ref().clone());
                continue;
            }
        };

        old_states_map.insert(id, s);
    }

    for control in controls {
        // States won't be present for things like groups.
        let old_state = match old_states_map.get(&control.id()) {
            Some(v) => *v,
            None => continue
        };

        let new_state = match new_states_map.get(&control.id()) {
            Some(v) => *v,
            None => {
                out.add_states(old_state.clone());
                continue;
            }
        };

        if old_state.current_value() == new_state.current_value() {
            out.add_states(old_state.clone());
            continue;
        }

        if !new_state.current_value().has_int32_value() {
            return Err(err_msg("No state is invalid"));
        }

        device.set_control_value(control, new_state.current_value().int32_value()).await?;

        out.add_states(new_state.clone());
    }
    
    Ok(out)
}


