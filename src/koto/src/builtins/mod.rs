use crate::{value, Runtime, Value, ValueList, ValueMap};
use koto_parser::vec4;
use std::{fs, path::Path, rc::Rc};

mod list;

#[macro_export]
macro_rules! single_arg_fn {
    ($map_name: ident, $fn_name: expr, $type: ident, $match_name: ident, $body: block) => {
        $map_name.add_fn($fn_name, |args| {
            if args.len() == 1 {
                match args.first().unwrap() {
                    $type($match_name) => $body
                    unexpected => {
                        return Err(format!(
                            "{}.{} only accepts a {} as its argument, found {}",
                            stringify!($map_name),
                            $fn_name,
                            stringify!($type),
                            value::type_as_string(unexpected)
                        ))
                    }
                }
            } else {
                Err(format!(
                    "{}.{} expects one argument, found {}",
                    stringify!($map_name),
                    $fn_name,
                    args.len()
                ))
            }
        });
    }
}

pub fn register<'a>(runtime: &mut Runtime<'a>) {
    use Value::*;

    let global = runtime.global_mut();

    list::register(global);

    {
        let mut math = ValueMap::new();

        macro_rules! math_fn_1 {
            ($fn:ident) => {
                math_fn_1!(stringify!($fn), $fn)
            };
            ($name:expr, $fn:ident) => {
                single_arg_fn!(math, $name, Number, n, { Ok(Number(n.$fn())) });
            };
        }

        math_fn_1!(abs);
        math_fn_1!(acos);
        math_fn_1!(asin);
        math_fn_1!(atan);
        math_fn_1!(ceil);
        math_fn_1!(cos);
        math_fn_1!(cosh);
        math_fn_1!("degrees", to_degrees);
        math_fn_1!(exp);
        math_fn_1!(exp2);
        math_fn_1!(floor);
        math_fn_1!(log10);
        math_fn_1!(log2);
        math_fn_1!(ln);
        math_fn_1!("radians", to_radians);
        math_fn_1!(recip);
        math_fn_1!(sin);
        math_fn_1!(sinh);
        math_fn_1!(sqrt);
        math_fn_1!(tan);
        math_fn_1!(tanh);

        global.add_map("math", math);
    }

    {
        let mut map = ValueMap::new();

        single_arg_fn!(map, "keys", Map, m, {
            Ok(List(Rc::new(ValueList::with_data(
                m.0.keys().map(|k| Str(k.clone())).collect::<Vec<_>>(),
            ))))
        });

        global.add_map("map", map);
    }

    {
        let mut string = ValueMap::new();

        single_arg_fn!(string, "escape", Str, s, {
            Ok(Str(Rc::new(s.escape_default().to_string())))
        });

        single_arg_fn!(string, "lines", Str, s, {
            Ok(List(Rc::new(ValueList::with_data(
                s.lines()
                    .map(|line| Str(Rc::new(line.to_string())))
                    .collect::<Vec<_>>(),
            ))))
        });

        global.add_map("string", string);
    }

    {
        let mut io = ValueMap::new();

        single_arg_fn!(io, "exists", Str, path, {
            Ok(Bool(Path::new(path.as_ref()).exists()))
        });

        single_arg_fn!(io, "read_string", Str, path, {
            {
                match fs::read_to_string(Path::new(path.as_ref())) {
                    Ok(result) => Ok(Str(Rc::new(result))),
                    Err(e) => Err(format!("Unable to read file {}: {}", path, e)),
                }
            }
        });

        global.add_map("io", io);
    }

    global.add_fn("assert", |args| {
        for value in args.iter() {
            match value {
                Bool(b) => {
                    if !b {
                        return Err("Assertion failed".to_string());
                    }
                }
                _ => return Err("assert only expects booleans as arguments".to_string()),
            }
        }
        Ok(Empty)
    });

    global.add_fn("assert_eq", |args| {
        if args.len() != 2 {
            Err(format!(
                "assert_eq expects two arguments, found {}",
                args.len()
            ))
        } else if args[0] == args[1] {
            Ok(Empty)
        } else {
            Err(format!(
                "Assertion failed, '{}' is not equal to '{}'",
                args[0], args[1]
            ))
        }
    });

    global.add_fn("assert_ne", |args| {
        if args.len() != 2 {
            Err(format!(
                "assert_ne expects two arguments, found {}",
                args.len()
            ))
        } else if args[0] != args[1] {
            Ok(Empty)
        } else {
            Err(format!(
                "Assertion failed, '{}' should not be equal to '{}'",
                args[0], args[1]
            ))
        }
    });

    global.add_fn("size", |args| {
        let mut arg_iter = args.iter();
        let first_arg_value = match arg_iter.next() {
            Some(arg) => arg,
            None => {
                return Err("Missing list as first argument for size".to_string());
            }
        };

        match first_arg_value {
            Empty => Ok(Number(0.0)),
            List(list) => Ok(Number(list.data().len() as f64)),
            Range { min, max } => Ok(Number((max - min) as f64)),
            unexpected => Err(format!(
                "size is only supported for lists and ranges, found {}",
                unexpected
            )),
        }
    });

    global.add_fn("number", |args| {
        let mut arg_iter = args.iter();
        let first_arg_value = match arg_iter.next() {
            Some(arg) => arg,
            None => {
                return Err("Missing list as first argument for size".to_string());
            }
        };

        match first_arg_value {
            Number(_) => Ok(first_arg_value.clone()),
            Str(s) => match s.parse::<f64>() {
                Ok(n) => Ok(Number(n)),
                Err(_) => Err(format!("Failed to convert '{}' into a Number", s)),
            },
            unexpected => Err(format!(
                "number is only supported for numbers and strings, found {}",
                unexpected
            )),
        }
    });

    global.add_fn("vec4", |args| {
        use vec4::Vec4 as V4;

        let result = match args {
            [] => V4::default(),
            [arg] => match arg {
                Number(n) => {
                    let n = *n as f32;
                    V4(n, n, n, n)
                }
                Vec4(v) => *v,
                List(list) => {
                    let mut v = V4::default();
                    for (i, value) in list.data().iter().take(4).enumerate() {
                        match value {
                            Number(n) => v[i] = *n as f32,
                            unexpected => {
                                return Err(format!(
                                    "vec4 only accepts Numbers as arguments, - found {}",
                                    unexpected
                                ))
                            }
                        }
                    }
                    v
                }
                unexpected => {
                    return Err(format!(
                        "vec4 only accepts a Number, Vec4, or List as first argument - found {}",
                        unexpected
                    ))
                }
            },
            _ => {
                let mut v = V4::default();
                for (i, arg) in args.iter().take(4).enumerate() {
                    match arg {
                        Number(n) => v[i] = *n as f32,
                        unexpected => {
                            return Err(format!(
                                "vec4 only accepts Numbers as arguments, \
                                     or Vec4 or List as first argument - found {}",
                                unexpected
                            ));
                        }
                    }
                }
                v
            }
        };

        Ok(Vec4(result))
    });

    global.add_fn("print", |args| {
        for value in args.iter() {
            print!("{}", value);
        }
        println!();
        Ok(Empty)
    });
}
