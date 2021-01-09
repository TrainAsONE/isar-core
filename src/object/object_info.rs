use crate::object::data_type::DataType;
use crate::object::object_id::ObjectId;
use crate::object::property::Property;
use serde_json::{json, Map, Value};

#[cfg_attr(test, derive(Clone))]
pub struct ObjectInfo {
    properties: Vec<Property>,
    property_names: Vec<String>,
    static_size: usize,
}

impl ObjectInfo {
    pub(crate) fn new(properties: Vec<Property>, property_names: Vec<String>) -> ObjectInfo {
        let static_size = Self::calculate_static_size(&properties);
        ObjectInfo {
            properties,
            property_names,
            static_size,
        }
    }

    fn calculate_static_size(properties: &[Property]) -> usize {
        let last_property = properties.last().unwrap();
        last_property.offset + last_property.data_type.get_static_size()
    }

    pub fn get_static_size(&self) -> usize {
        self.static_size
    }

    pub fn get_property(&self, index: usize) -> Option<Property> {
        self.properties.get(index).copied()
    }

    pub fn iter_properties(&self) -> impl Iterator<Item = (&String, Property)> {
        self.property_names
            .iter()
            .zip(self.properties.iter().copied())
    }

    pub fn get_property_by_name(&self, property_name: &str) -> Option<Property> {
        self.property_names
            .iter()
            .position(|n| n == property_name)
            .map(|index| self.properties.get(index).unwrap())
            .copied()
    }

    pub fn entry_to_json(&self, key: &[u8], object: &[u8], primitive_null: bool) -> Value {
        let mut object_map = Map::new();

        let oid = ObjectId::from_bytes(key);
        object_map.insert("id".to_string(), json!(oid.to_string()));

        let properties = self.property_names.iter().zip(self.properties.iter());
        for (name, property) in properties {
            let value =
                if primitive_null && property.data_type.is_static() && property.is_null(object) {
                    Value::Null
                } else {
                    match property.data_type {
                        DataType::Byte => json!(property.get_byte(object)),
                        DataType::Int => json!(property.get_int(object)),
                        DataType::Float => json!(property.get_float(object)),
                        DataType::Long => json!(property.get_long(object)),
                        DataType::Double => json!(property.get_double(object)),
                        DataType::String => json!(property.get_string(object)),
                        DataType::ByteList => json!(property.get_byte_list(object)),
                        DataType::IntList => json!(property.get_int_list(object)),
                        DataType::FloatList => json!(property.get_float_list(object)),
                        DataType::LongList => json!(property.get_float_list(object)),
                        DataType::DoubleList => json!(property.get_double_list(object)),
                        DataType::StringList => json!(property.get_string_list(object)),
                    }
                };
            object_map.insert(name.to_string(), value);
        }
        json!(object_map)
    }

    pub fn verify_object(&self, object: &[u8]) -> bool {
        let alignment = object.as_ref().as_ptr() as usize - ObjectId::get_size();
        if alignment % 8 != 0 {
            return false;
        }
        let check_padding = |index: usize, count: usize| -> bool {
            if object.len() < index + count {
                return false;
            }
            for padding_byte in &object[index..index + count] {
                if *padding_byte != 0 {
                    return false;
                }
            }
            true
        };

        if (ObjectId::get_size() + object.len()) % 8 != 0 {
            return false;
        }

        let mut static_offset = 0;
        let mut dynamic_offset = self.static_size;
        for property in &self.properties {
            let required_padding = property.offset - static_offset;
            if !check_padding(static_offset, required_padding) {
                return false;
            }
            static_offset += required_padding;

            if property.offset != static_offset {
                return false;
            }
            static_offset += property.data_type.get_static_size();

            if property.data_type.is_dynamic() && !property.is_null(object) {
                let pos = property.get_dynamic_position(object).unwrap();
                let alignment_wrong = (dynamic_offset + ObjectId::get_size())
                    % property.data_type.get_element_size()
                    != 0;
                if pos.offset as usize != dynamic_offset || alignment_wrong {
                    return false;
                }

                if property.data_type == DataType::StringList {
                    let list_positions = property.get_dynamic_positions(object).unwrap();
                    let last_with_length = list_positions.iter().rev().find(|p| p.length != 0);
                    if let Some(last_pos) = last_with_length {
                        dynamic_offset += last_pos.length as usize;
                    }
                } else {
                    dynamic_offset += pos.length as usize * property.data_type.get_element_size();
                }
            }
        }

        if static_offset != self.static_size {
            return false;
        }

        let required_padding = (8 - (dynamic_offset + ObjectId::get_size()) % 8) % 8;
        if !check_padding(dynamic_offset, required_padding as usize) {
            return false;
        }

        dynamic_offset + required_padding == object.len()
    }
}
#[cfg(test)]
mod tests {
    use crate::object::data_type::DataType;
    use crate::object::object_info::ObjectInfo;
    use crate::object::property::Property;

    #[test]
    fn test_calculate_static_size() {
        let properties1 = vec![
            Property::new(DataType::Byte, 0),
            Property::new(DataType::Int, 2),
        ];
        let properties2 = vec![
            Property::new(DataType::Byte, 0),
            Property::new(DataType::String, 1),
            Property::new(DataType::ByteList, 9),
            Property::new(DataType::Double, 9),
        ];

        assert_eq!(ObjectInfo::calculate_static_size(&properties1), 6);
        assert_eq!(ObjectInfo::calculate_static_size(&properties2), 17);
    }

    #[test]
    fn test_verify_object() {
        /*let oi = ObjectInfo::new(vec![Property::new(DataType::Bool, 0)]);
        assert!(oi.verify_object(&[1, 0])); // correct end padding
        assert!(!oi.verify_object(&[1])); // wrong end padding
        assert!(!oi.verify_object(&[1, 6])); // wrong end padding

        let oi = ObjectInfo::new(vec![Property::new(DataType::Bool, 1)]);
        assert!(oi.verify_object(&[0, 1])); // correct start padding
        assert!(!oi.verify_object(&[5, 1])); // wrong start padding

        let oi = ObjectInfo::new(vec![
            Property::new(DataType::Bool, 0),
            Property::new(DataType::Int, 2),
        ]);
        assert!(oi.verify_object(&[1, 0, 1, 0, 0, 0, 0, 0, 0, 0])); // correct length
        assert!(!oi.verify_object(&[1, 0])); // missing property
        assert!(!oi.verify_object(&[1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        // too long

        let oi = ObjectInfo::new(vec![Property::new(DataType::BoolList, 2)]);
        assert!(oi.verify_object(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0])); // null list
        assert!(oi.verify_object(&[0, 0, 10, 0, 0, 0, 0, 0, 0, 0])); // empty list
        assert!(!oi.verify_object(&[0, 0, 7, 0, 0, 0, 0, 0, 0, 0])); // offset in static area
        assert!(!oi.verify_object(&[0, 0, 11, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]));
        // offset leaves hole
        assert!(!oi.verify_object(&[0, 0, 10, 0, 0, 0, 9, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1]));
        // missing data

        let oi = ObjectInfo::new(vec![Property::new(DataType::IntList, 2)]);
        assert!(oi.verify_object(&[0, 0, 10, 0, 0, 0, 2, 0, 0, 0, 20, 0, 0, 0, 21, 0, 0, 0]));
        // correct list

        let oi = ObjectInfo::new(vec![Property::new(DataType::StringList, 2)]);
        assert!(oi.verify_object(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0])); // null string list
        assert!(oi.verify_object(&[0, 0, 10, 0, 0, 0, 0, 0, 0, 0])); // empty string list
        assert!(oi.verify_object(&[0, 0, 10, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        // string list with null entry
        assert!(!oi.verify_object(&[0, 0, 7, 0, 0, 0, 0, 0, 0, 0])); // offset in static area
        assert!(oi.verify_object(&[0, 0, 10, 0, 0, 0, 1, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0])); // offset in data pos area
        assert!(oi.verify_object(&[
            0, 0, 10, 0, 0, 0, 2, 0, 0, 0, 12, 0, 0, 0, 1, 0, 0, 0, 12, 0, 0, 0, 2, 0, 0, 0, 44, 0,
            45, 46
        ])); // offset leaves hole*/
    }
}
