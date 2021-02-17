use crate::error::{illegal_arg, Result};
use crate::object::isar_object::{IsarObject, Property};
use crate::query::fast_wild_compare::fast_wild_compare_portable;
use enum_dispatch::enum_dispatch;
use paste::paste;

#[enum_dispatch]
#[derive(Clone)]
pub enum Filter {
    IsNull(IsNullCond),

    ByteBetween(ByteBetweenCond),
    IntBetween(IntBetweenCond),
    LongBetween(LongBetweenCond),
    FloatBetween(FloatBetweenCond),
    DoubleBetween(DoubleBetweenCond),

    ByteListContains(ByteListContainsCond),
    IntListContains(IntListContainsCond),
    LongListContains(LongListContainsCond),

    StringEqual(StringEqualCond),
    StringStartsWith(StringStartsWithCond),
    StringEndsWith(StringEndsWithCond),
    StringLike(StringLikeCond),

    StringListContains(StringListContainsCond),

    And(AndCond),
    Or(OrCond),
    Not(NotCond),
    Static(StaticCond),
}

#[enum_dispatch(Filter)]
pub trait Condition {
    fn evaluate(&self, object: IsarObject) -> bool;
}

#[derive(Clone)]
pub struct IsNullCond {
    property: Property,
}

impl Condition for IsNullCond {
    fn evaluate(&self, object: IsarObject) -> bool {
        object.is_null(self.property)
    }
}

impl IsNullCond {
    pub fn filter(property: Property) -> Filter {
        Filter::IsNull(Self { property })
    }
}

#[macro_export]
macro_rules! filter_between_struct {
    ($name:ident, $data_type:ident, $type:ty) => {
        paste! {
            #[derive(Clone)]
            pub struct [<$name Cond>] {
                upper: $type,
                lower: $type,
                property: Property,
            }

            impl [<$name Cond>] {
                pub fn filter(property: Property, lower: $type, upper: $type) -> Result<Filter> {
                    if property.data_type == crate::object::data_type::DataType::$data_type {
                        Ok(Filter::$name(Self {
                            property,
                            lower,
                            upper,
                        }))
                    } else {
                        illegal_arg("Property does not support this filter.")
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! primitive_filter_between {
    ($name:ident, $data_type:ident, $type:ty, $prop_accessor:ident) => {
        filter_between_struct!($name, $data_type, $type);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject) -> bool {
                    let val = object.$prop_accessor(self.property);
                    self.lower <= val && self.upper >= val
                }
            }
        }
    };
}

#[macro_export]
macro_rules! float_filter_between {
    ($name:ident, $data_type:ident, $type:ty, $prop_accessor:ident) => {
        filter_between_struct!($name, $data_type, $type);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject) -> bool {
                    let val = object.$prop_accessor(self.property);
                    if self.upper.is_nan() {
                        self.lower.is_nan() && val.is_nan()
                    } else if self.lower.is_nan() {
                        self.upper >= val || val.is_nan()
                    } else {
                        self.lower <= val && self.upper >= val
                    }
                }
            }
        }
    };
}

primitive_filter_between!(ByteBetween, Byte, u8, read_byte);
primitive_filter_between!(IntBetween, Int, i32, read_int);
primitive_filter_between!(LongBetween, Long, i64, read_long);
float_filter_between!(FloatBetween, Float, f32, read_float);
float_filter_between!(DoubleBetween, Double, f64, read_double);

#[macro_export]
macro_rules! filter_not_equal_struct {
    ($name:ident, $data_type:ident, $type:ty) => {
        paste! {
            #[derive(Clone)]
            pub struct [<$name Cond>] {
                value: $type,
                property: Property,
            }

            impl [<$name Cond>] {
                pub fn filter(property: Property, value: $type) -> Result<Filter> {
                    if property.data_type == crate::object::data_type::DataType::$data_type {
                        Ok(Filter::$name(Self { property, value }))
                    } else {
                        illegal_arg("Property does not support this filter.")
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! primitive_list_filter {
    ($name:ident, $data_type:ident, $type:ty, $prop_accessor:ident) => {
        filter_not_equal_struct!($name, $data_type, $type);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject) -> bool {
                    let list = object.$prop_accessor(self.property);
                    if let Some(list) = list {
                        list.contains(&self.value)
                    } else {
                        false
                    }
                }
            }
        }
    };
}

primitive_list_filter!(ByteListContains, Byte, u8, read_byte_list);
primitive_list_filter!(IntListContains, Int, i32, read_int_list);
primitive_list_filter!(LongListContains, Long, i64, read_long_list);

#[macro_export]
macro_rules! string_filter_struct {
    ($name:ident) => {
        paste! {
            #[derive(Clone)]
            pub struct [<$name Cond>] {
                property: Property,
                value: Option<String>,
                case_sensitive: bool,
            }

            impl [<$name Cond>] {
                pub fn filter(
                    property: Property,
                    value: Option<&str>,
                    case_sensitive: bool,
                ) -> Result<Filter> {
                    let value = if case_sensitive {
                        value.map(|s| s.to_string())
                    } else {
                        value.map(|s| s.to_lowercase())
                    };
                    if property.data_type == crate::object::data_type::DataType::String {
                        Ok(Filter::$name([<$name Cond>] {
                            property,
                            value,
                            case_sensitive,
                        }))
                    } else {
                        illegal_arg("Property does not support this filter.")
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! string_filter {
    ($name:ident) => {
        string_filter_struct!($name);
        paste! {
            impl Condition for [<$name Cond>] {
                fn evaluate(&self, object: IsarObject) -> bool {
                    let other_str = object.read_string(self.property);
                    if let (Some(filter_str), Some(other_str)) = (self.value.as_ref(), other_str) {
                        if self.case_sensitive {
                            string_filter!($name filter_str, other_str)
                        } else {
                            let lowercase_string = other_str.to_lowercase();
                            let lowercase_str = &lowercase_string;
                            string_filter!($name filter_str, lowercase_str)
                        }
                    } else {
                        self.value.is_none() && other_str.is_none()
                    }
                }
            }
        }
    };

    (StringEqual $filter_str:ident, $other_str:ident) => {
        $filter_str == $other_str
    };

    (StringNotEqual $filter_str:ident, $other_str:ident) => {
        $filter_str != $other_str
    };

    (StringStartsWith $filter_str:ident, $other_str:ident) => {
        $other_str.starts_with($filter_str)
    };

    (StringEndsWith $filter_str:ident, $other_str:ident) => {
        $other_str.ends_with($filter_str)
    };

    (StringLike $filter_str:ident, $other_str:ident) => {
        fast_wild_compare_portable($other_str, $filter_str)
    };
}

string_filter!(StringEqual);
string_filter!(StringStartsWith);
string_filter!(StringEndsWith);
string_filter!(StringLike);

string_filter_struct!(StringListContains);

impl Condition for StringListContainsCond {
    fn evaluate(&self, object: IsarObject) -> bool {
        let list = object.read_string_list(self.property);
        if let Some(list) = list {
            list.contains(&self.value.as_deref())
        } else {
            false
        }
    }
}

#[derive(Clone)]
pub struct AndCond {
    filters: Vec<Filter>,
}

impl Condition for AndCond {
    fn evaluate(&self, object: IsarObject) -> bool {
        for filter in &self.filters {
            if !filter.evaluate(object) {
                return false;
            }
        }
        true
    }
}

impl AndCond {
    pub fn filter(filters: Vec<Filter>) -> Filter {
        Filter::And(AndCond { filters })
    }
}

#[derive(Clone)]
pub struct OrCond {
    filters: Vec<Filter>,
}

impl Condition for OrCond {
    fn evaluate(&self, object: IsarObject) -> bool {
        for filter in &self.filters {
            if filter.evaluate(object) {
                return true;
            }
        }
        false
    }
}

impl OrCond {
    pub fn filter(filters: Vec<Filter>) -> Filter {
        Filter::Or(OrCond { filters })
    }
}

#[derive(Clone)]
pub struct NotCond {
    filter: Box<Filter>,
}

impl Condition for NotCond {
    fn evaluate(&self, object: IsarObject) -> bool {
        !self.filter.evaluate(object)
    }
}

impl NotCond {
    pub fn filter(filter: Filter) -> Filter {
        Filter::Not(NotCond {
            filter: Box::new(filter),
        })
    }
}

#[derive(Clone)]
pub struct StaticCond {
    value: bool,
}

impl Condition for StaticCond {
    fn evaluate(&self, _: IsarObject) -> bool {
        self.value
    }
}

impl StaticCond {
    pub fn filter(value: bool) -> Filter {
        Filter::Static(StaticCond { value })
    }
}
