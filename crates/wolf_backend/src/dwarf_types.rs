//! DWARF 5 section 5 source-type entries (also representable in our DWARF 4 CU).
use std::collections::HashMap;

use gimli::constants as dw;
use gimli::write::{AttributeValue as A, DwarfUnit, UnitEntryId};
use wolf_wir::debug::{
    DebugDiscriminant, DebugEncoding, DebugKind, DebugMember, DebugType, DebugTypeData,
};

pub(crate) struct Types {
    entries: HashMap<DebugType, UnitEntryId>,
    error_tags: Vec<String>,
}

impl Types {
    pub(crate) fn new(error_tags: Vec<String>) -> Self {
        Self {
            entries: HashMap::new(),
            error_tags,
        }
    }

    pub(crate) fn emit(&mut self, dwarf: &mut DwarfUnit, ty: &DebugType) -> UnitEntryId {
        if let Some(id) = self.entries.get(ty) {
            return *id;
        }
        let tag = match &ty.kind {
            DebugKind::Base(_) => dw::DW_TAG_base_type,
            DebugKind::Pointer(_) => dw::DW_TAG_pointer_type,
            DebugKind::Reference(_) => dw::DW_TAG_reference_type,
            DebugKind::Enum(_) => dw::DW_TAG_enumeration_type,
            DebugKind::Struct(_) | DebugKind::Variants(_) | DebugKind::Opaque => {
                dw::DW_TAG_structure_type
            }
        };
        let id = dwarf.unit.add(dwarf.unit.root(), tag);
        self.entries.insert(ty.clone(), id);
        dwarf
            .unit
            .get_mut(id)
            .set(dw::DW_AT_name, A::String(ty.name.as_bytes().to_vec()));
        if matches!(&ty.kind, DebugKind::Opaque) {
            dwarf
                .unit
                .get_mut(id)
                .set(dw::DW_AT_declaration, A::Flag(true));
            return id;
        }
        dwarf
            .unit
            .get_mut(id)
            .set(dw::DW_AT_byte_size, A::Udata(ty.size));
        match &ty.kind {
            DebugKind::Base(enc) => {
                let enc = match enc {
                    DebugEncoding::Signed => dw::DW_ATE_signed,
                    DebugEncoding::Unsigned => dw::DW_ATE_unsigned,
                    DebugEncoding::Float => dw::DW_ATE_float,
                    DebugEncoding::Boolean => dw::DW_ATE_boolean,
                    DebugEncoding::UnsignedChar => dw::DW_ATE_unsigned_char,
                };
                dwarf
                    .unit
                    .get_mut(id)
                    .set(dw::DW_AT_encoding, A::Encoding(enc));
            }
            DebugKind::Pointer(to) | DebugKind::Reference(to) => {
                let target = self.emit(dwarf, to);
                dwarf
                    .unit
                    .get_mut(id)
                    .set(dw::DW_AT_type, A::UnitRef(target));
            }
            DebugKind::Struct(fields) => {
                for field in fields {
                    self.member(dwarf, id, field);
                }
            }
            DebugKind::Enum(variants) => {
                let scalar = self.tag_type(dwarf);
                dwarf
                    .unit
                    .get_mut(id)
                    .set(dw::DW_AT_type, A::UnitRef(scalar));
                for (name, value) in variants {
                    let entry = dwarf.unit.add(id, dw::DW_TAG_enumerator);
                    let die = dwarf.unit.get_mut(entry);
                    die.set(dw::DW_AT_name, A::String(name.as_bytes().to_vec()));
                    die.set(dw::DW_AT_const_value, A::Sdata(*value));
                }
            }
            DebugKind::Variants(variants) => {
                // DWARF 5 5.7.10: the discriminant is a member owned by
                // the variant part, and DW_AT_discr refers to that member.
                let part = dwarf.unit.add(id, dw::DW_TAG_variant_part);
                let scalar = self.tag_type(dwarf);
                let discr = dwarf.unit.add(part, dw::DW_TAG_member);
                let die = dwarf.unit.get_mut(discr);
                die.set(dw::DW_AT_name, A::String(b"__tag".to_vec()));
                die.set(dw::DW_AT_type, A::UnitRef(scalar));
                die.set(dw::DW_AT_data_member_location, A::Udata(0));
                die.set(dw::DW_AT_artificial, A::Flag(true));
                dwarf
                    .unit
                    .get_mut(part)
                    .set(dw::DW_AT_discr, A::UnitRef(discr));
                for variant in variants {
                    let entry = dwarf.unit.add(part, dw::DW_TAG_variant);
                    let value = match &variant.discriminant {
                        DebugDiscriminant::Value(value) => Some(*value),
                        DebugDiscriminant::ErrorTag(name) => {
                            // All executable tags were assigned before the sink
                            // was called. Append only unconstructed source cases,
                            // without renumbering a tag executable code can hold.
                            let index = match self.error_tags.iter().position(|tag| tag == name) {
                                Some(index) => index,
                                None => {
                                    self.error_tags.push(name.clone());
                                    self.error_tags.len() - 1
                                }
                            };
                            Some(index as i64 + 1)
                        }
                        DebugDiscriminant::Default => None,
                    };
                    if let Some(value) = value {
                        dwarf
                            .unit
                            .get_mut(entry)
                            .set(dw::DW_AT_discr_value, A::Sdata(value));
                    }
                    // A named payload structure carries the source variant name.
                    // Its fields use offsets from the enclosing value's start;
                    // the variant's member starts there, not at the tag's end.
                    let payload = DebugTypeData::new(
                        format!("{}::{}", ty.name, variant.name),
                        ty.size,
                        DebugKind::Struct(variant.members.clone()),
                    );
                    self.member(
                        dwarf,
                        entry,
                        &DebugMember {
                            name: variant.name.clone(),
                            offset: 0,
                            ty: payload,
                        },
                    );
                }
            }
            DebugKind::Opaque => unreachable!("handled above"),
        }
        id
    }

    fn tag_type(&mut self, dwarf: &mut DwarfUnit) -> UnitEntryId {
        self.emit(
            dwarf,
            &DebugTypeData::new("int", 8, DebugKind::Base(DebugEncoding::Signed)),
        )
    }

    fn member(&mut self, dwarf: &mut DwarfUnit, owner: UnitEntryId, member: &DebugMember) {
        let ty = self.emit(dwarf, &member.ty);
        let id = dwarf.unit.add(owner, dw::DW_TAG_member);
        let die = dwarf.unit.get_mut(id);
        die.set(dw::DW_AT_name, A::String(member.name.as_bytes().to_vec()));
        die.set(dw::DW_AT_type, A::UnitRef(ty));
        die.set(dw::DW_AT_data_member_location, A::Udata(member.offset));
    }
}
