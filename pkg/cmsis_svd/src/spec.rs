use std::collections::HashMap;

use common::errors::*;

use crate::helpers::*;

pub struct Device<'a> {
    pub register_properties_group: RegisterPropertiesGroup,
    pub peripherals: Vec<Peripheral<'a>>,
}

impl<'a> Device<'a> {
    pub fn parse_from(element: &'a xml::Element) -> Result<Self> {
        let mut peripherals = {
            let element = get_child_named(element, "peripherals")?;

            let mut out = vec![];
            out.reserve_exact(element.content.len());

            let mut seen_peripherals = HashMap::new();

            for peripheral in element.children() {
                if peripheral.name != "peripheral" {
                    return Err(err_msg("Unknown peripheral tag"));
                }

                let mut inst = Peripheral::create(peripheral)?;
                if let Some(name) = inst.derived_from {
                    let parent_index = *seen_peripherals
                        .get(name)
                        .ok_or_else(|| err_msg("Unknown parent peripheral"))?;
                    // inst = inst.inherit(&out[parent_index]);
                }

                seen_peripherals.insert(inst.name, out.len());
                out.push(inst);
            }

            out
        };

        let register_properties_group = RegisterPropertiesGroup::create(element)?;

        Ok(Self {
            peripherals,
            register_properties_group,
        })
    }
}

pub struct Peripheral<'a> {
    pub name: &'a str,
    pub base_address: usize,
    pub derived_from: Option<&'a str>,
    pub interrupts: Vec<Interrupt<'a>>,
    pub register_properties_group: RegisterPropertiesGroup,
    pub registers: Vec<ClusterOrRegister<'a>>,
}

impl<'a> Peripheral<'a> {
    fn create(element: &'a xml::Element) -> Result<Self> {
        let name = inner_text(get_child_named(element, "name")?)?;
        let base_address = decode_number(inner_text(get_child_named(element, "baseAddress")?)?)?;

        let derived_from = element.attributes.get("derivedFrom").map(|s| s.as_str());

        let mut interrupts = vec![];
        for element in element.children().find(|e| e.name == "interrupt") {
            let name = inner_text(get_child_named(element, "name")?)?;
            let value = decode_number(inner_text(get_child_named(element, "value")?)?)?;

            interrupts.push(Interrupt { name, value });
        }

        let mut registers = vec![];
        if let Some(element) = get_optional_child_named(element, "registers")? {
            registers = ClusterOrRegister::create_children(element)?;
        }

        let register_properties_group = RegisterPropertiesGroup::create(element)?;

        Ok(Self {
            name,
            base_address,
            derived_from,
            interrupts,
            register_properties_group,
            registers,
        })
    }

    pub fn inherit(mut self, parent: &Self) -> Self {
        assert!(!parent.derived_from.is_some());
        // TODO: Assert that the name of the parent is the name of our derived_from
        // field.

        self.interrupts.extend(parent.interrupts.iter().cloned());
        self.registers.extend(parent.registers.iter().cloned());

        Self {
            name: self.name,
            base_address: self.base_address,
            derived_from: None,
            interrupts: self.interrupts,
            register_properties_group: self
                .register_properties_group
                .clone()
                .inherit(&parent.register_properties_group),
            registers: self.registers,
        }
    }
}

#[derive(Clone)]
pub struct Interrupt<'a> {
    pub name: &'a str,
    pub value: usize,
}

#[derive(Clone)]
pub enum ClusterOrRegister<'a> {
    Cluster(Cluster<'a>),
    Register(Register<'a>),
}

impl<'a> ClusterOrRegister<'a> {
    fn create_children(element: &'a xml::Element) -> Result<Vec<Self>> {
        let mut children = vec![];
        for element in element.children() {
            if element.name == "register" {
                let register = Register::create(element)?;
                children.push(ClusterOrRegister::Register(register));
            } else if element.name == "cluster" {
                let cluster = Cluster::create(element)?;
                children.push(ClusterOrRegister::Cluster(cluster));
            }
        }

        Ok(children)
    }
}

#[derive(Clone)]
pub struct Cluster<'a> {
    pub name: &'a str,
    pub description: &'a str, // OPTIONAL
    pub address_offset: usize,
    pub dim_element_group: Option<DimElementGroup<'a>>,
    pub children: Vec<ClusterOrRegister<'a>>,
}

impl<'a> Cluster<'a> {
    fn create(element: &'a xml::Element) -> Result<Self> {
        let name = inner_text(get_child_named(element, "name")?)?;
        let description = inner_text(get_child_named(element, "description")?)?;
        let address_offset =
            decode_number(inner_text(get_child_named(element, "addressOffset")?)?)?;
        let dim_element_group = DimElementGroup::create(element)?;
        let children = ClusterOrRegister::create_children(element)?;
        Ok(Self {
            name,
            description,
            address_offset,
            dim_element_group,
            children,
        })
    }
}

#[derive(Clone)]
pub struct DimElementGroup<'a> {
    pub dim: usize,
    pub dim_increment: usize,

    /// Examples: 'A,B,C,D,E,Z'
    /// Or '3-6'
    pub dim_index: Option<&'a str>,
}

impl<'a> DimElementGroup<'a> {
    fn create(element: &'a xml::Element) -> Result<Option<Self>> {
        let dim = match get_optional_child_named(element, "dim")? {
            Some(v) => decode_number(inner_text(v)?)?,
            None => {
                return Ok(None);
            }
        };

        let dim_increment = decode_number(inner_text(get_child_named(element, "dimIncrement")?)?)?;

        let dim_index = match get_optional_child_named(element, "dimIndex")? {
            Some(v) => Some(inner_text(v)?),
            None => None,
        };

        Ok(Some(Self {
            dim,
            dim_increment,
            dim_index,
        }))
    }
}

#[derive(Clone)]
pub struct Register<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub address_off: usize,
    pub properties: RegisterPropertiesGroup,
    pub dim_element_group: Option<DimElementGroup<'a>>,

    pub alternative_register: Option<&'a str>,

    // NOTE: We don't distinguish between a missing <fields> element and an empty <fields> element.
    pub fields: Vec<Field<'a>>,
}

impl<'a> Register<'a> {
    fn create(element: &'a xml::Element) -> Result<Self> {
        let name = inner_text(get_child_named(element, "name")?)?;

        let description = inner_text(get_child_named(element, "description")?)?;
        let address_off = decode_number(inner_text(get_child_named(element, "addressOffset")?)?)?;

        let properties = RegisterPropertiesGroup::create(element)?;

        let dim_element_group = DimElementGroup::create(element)?;

        let mut fields = vec![];
        if let Some(element) = get_optional_child_named(element, "fields")? {
            for element in element.children() {
                if element.name != "field" {
                    return Err(err_msg("Unknown field element"));
                }

                fields.push(Field::create(element)?);
            }
        }

        let mut alternative_register = None;
        if let Some(element) = get_optional_child_named(element, "alternateRegister")? {
            alternative_register = Some(inner_text(element)?);
        }

        Ok(Self {
            name,
            description,
            address_off,
            properties,
            dim_element_group,
            alternative_register,
            fields,
        })
    }
}

#[derive(Clone, Debug)]
pub struct RegisterPropertiesGroup {
    pub size: Option<usize>,
    pub access: Option<RegisterAccess>,
    // pub protection
    pub reset_value: Option<usize>,
    pub reset_mask: Option<usize>,
}

impl RegisterPropertiesGroup {
    fn create(element: &xml::Element) -> Result<Self> {
        let size = match get_optional_child_named(element, "size")? {
            Some(v) => Some(decode_number(inner_text(v)?)?),
            None => None,
        };

        let access = match get_optional_child_named(element, "access")? {
            Some(v) => Some(RegisterAccess::from(inner_text(v)?)?),
            None => None,
        };

        let reset_value = match get_optional_child_named(element, "resetValue")? {
            Some(v) => Some(decode_number(inner_text(v)?)?),
            None => None,
        };

        let reset_mask = match get_optional_child_named(element, "resetMask")? {
            Some(v) => Some(decode_number(inner_text(v)?)?),
            None => None,
        };

        Ok(Self {
            size,
            access,
            reset_mask,
            reset_value,
        })
    }

    pub fn inherit(self, parent: &RegisterPropertiesGroup) -> Self {
        Self {
            size: self.size.or(parent.size.clone()),
            access: self.access.or(parent.access.clone()),
            reset_value: self.reset_value.or(parent.reset_value.clone()),
            reset_mask: self.reset_mask.or(parent.reset_mask.clone()),
        }
    }

    pub fn resolve(&self) -> Result<ResolvedRegisterPropertiesGroup> {
        Ok(ResolvedRegisterPropertiesGroup {
            size: self.size.ok_or_else(|| err_msg("Unknown register size"))?,
            access: self
                .access
                .ok_or_else(|| err_msg("Unknown register access"))?,
            reset_value: self
                .reset_value
                .ok_or_else(|| err_msg("Unknown register reset_value"))?,
            reset_mask: self
                .reset_mask
                .ok_or_else(|| err_msg("Unknown register reset_mask"))?,
        })
    }
}

pub struct ResolvedRegisterPropertiesGroup {
    pub size: usize,
    pub access: RegisterAccess,
    pub reset_value: usize,
    pub reset_mask: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RegisterAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
    ReadWriteOnce,
}

impl RegisterAccess {
    // TODO: Make private
    fn from(value: &str) -> Result<Self> {
        Ok(match value {
            "read-only" => Self::ReadOnly,
            "write-only" => Self::WriteOnly,
            "read-write" => Self::ReadWrite,
            "read-writeonce" => Self::ReadWriteOnce,
            _ => {
                return Err(format_err!("Unknown register access: {}", value));
            }
        })
    }

    pub fn can_read(&self) -> bool {
        match self {
            Self::ReadOnly | Self::ReadWrite | Self::ReadWriteOnce => true,
            _ => false,
        }
    }

    pub fn can_write(&self) -> bool {
        match self {
            Self::WriteOnly | Self::ReadWrite | Self::ReadWriteOnce => true,
            _ => false,
        }
    }
}

#[derive(Clone)]
pub struct Field<'a> {
    pub name: &'a str,
    pub lsb: usize,
    pub msb: usize,
    pub read_enumerated_values: Option<Vec<EnumeratedValue<'a>>>,
    pub write_enumerated_values: Option<Vec<EnumeratedValue<'a>>>,
}

impl<'a> Field<'a> {
    fn create(element: &'a xml::Element) -> Result<Self> {
        let name = inner_text(get_child_named(element, "name")?)?;
        let lsb = decode_number(inner_text(get_child_named(element, "lsb")?)?)?;
        let msb = decode_number(inner_text(get_child_named(element, "msb")?)?)?;

        let mut read_enumerated_values = None;
        let mut write_enumerated_values = None;

        for element in element.children().filter(|e| e.name == "enumeratedValues") {
            let usage = match get_optional_child_named(element, "usage")? {
                Some(e) => EnumeratedValuesUsage::from(inner_text(e)?)?,
                None => EnumeratedValuesUsage::ReadWrite,
            };

            let mut values = vec![];
            for element in element.children().filter(|e| e.name == "enumeratedValue") {
                values.push(EnumeratedValue::create(element)?);
            }

            match usage {
                EnumeratedValuesUsage::Read => {
                    if read_enumerated_values.is_some() {
                        return Err(err_msg("Duplicate enum values"));
                    }

                    read_enumerated_values = Some(values);
                }
                EnumeratedValuesUsage::Write => {
                    if write_enumerated_values.is_some() {
                        return Err(err_msg("Duplicate enum values"));
                    }

                    write_enumerated_values = Some(values);
                }
                EnumeratedValuesUsage::ReadWrite => {
                    if read_enumerated_values.is_some() || read_enumerated_values.is_some() {
                        return Err(err_msg("Duplicate enum values"));
                    }

                    read_enumerated_values = Some(values.clone());
                    write_enumerated_values = Some(values);
                }
            }
        }

        Ok(Self {
            name,
            lsb,
            msb,
            read_enumerated_values,
            write_enumerated_values,
        })
    }
}

#[derive(Clone)]
pub enum EnumeratedValuesUsage {
    Read,
    Write,
    ReadWrite,
}

impl EnumeratedValuesUsage {
    fn from(value: &str) -> Result<Self> {
        Ok(match value {
            "read" => Self::Read,
            "write" => Self::Write,
            "read-write" => Self::ReadWrite,
            _ => {
                return Err(err_msg("Unknown enumerated values usage"));
            }
        })
    }
}

#[derive(Clone, PartialEq)]
pub struct EnumeratedValue<'a> {
    pub name: &'a str,
    pub desc: &'a str,
    pub value: usize,
}

impl<'a> EnumeratedValue<'a> {
    fn create(element: &'a xml::Element) -> Result<Self> {
        Ok(Self {
            name: inner_text(get_child_named(element, "name")?)?,
            desc: inner_text(get_child_named(element, "description")?)?,
            value: decode_number(inner_text(get_child_named(element, "value")?)?)?,
        })
    }
}
