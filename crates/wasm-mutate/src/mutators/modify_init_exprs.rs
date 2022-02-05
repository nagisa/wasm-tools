//! This mutator modifies the constant initializer expressions between various valid forms in
//! entities which require constant initializers.

use crate::mutators::translate::{self, Item, Translator};
use crate::{Error, Mutator, Result};

use rand::Rng;
use wasm_encoder::{ElementSection, GlobalSection, Instruction};
use wasmparser::{ElementSectionReader, GlobalSectionReader, InitExpr, Operator};

#[derive(Copy, Clone)]
pub struct InitExpressionMutator(pub Item);

struct InitTranslator {
    new_value: u128,
    replace_for_ty: Option<wasmparser::Type>,
}

impl Translator for InitTranslator {
    fn as_obj(&mut self) -> &mut dyn Translator {
        self
    }

    fn translate_element_table_offset(&mut self, e: &InitExpr<'_>) -> Result<Instruction<'static>> {
        let mut reader = e.get_operators_reader();
        let op = reader.read()?;
        if let Operator::I32Const { .. } = op {
            return Err(Error::no_mutations_applicable());
        }
        let new_op = Instruction::I32Const(0);
        println!("replacing offset {:?} with {:?}", op, new_op);
        Ok(new_op)
    }

    fn translate_init_expr(&mut self, e: &InitExpr<'_>) -> Result<Instruction<'static>> {
        use {wasmparser::Type as T, Instruction as I};
        let mut reader = e.get_operators_reader();
        let op = reader.read()?;
        if let Operator::RefNull { .. } = op {
            // Can't improve on this any more.
            return Err(Error::no_mutations_applicable());
        }
        let new_op = match self.replace_for_ty {
            Some(T::I32) => I::I32Const(self.new_value as u32 as _),
            Some(T::I64) => I::I64Const(self.new_value as u64 as _),
            Some(T::F32) => I::F32Const(f32::from_bits(self.new_value as u32)),
            Some(T::F64) => I::F64Const(f64::from_bits(self.new_value as u64)),
            Some(T::V128) => I::V128Const(self.new_value as i128),
            Some(T::FuncRef) => I::RefNull(wasm_encoder::ValType::FuncRef),
            Some(T::ExternRef) => I::RefNull(wasm_encoder::ValType::ExternRef),
            // wasm_encoder does not support ExnRef stuff yet.
            Some(T::ExnRef) => return Err(Error::no_mutations_applicable()),
            Some(T::Func) => return Err(Error::no_mutations_applicable()),
            Some(T::EmptyBlockType) => return Err(Error::no_mutations_applicable()),
            None => return translate::init_expr(self.as_obj(), e),
        };
        println!("replacing {:?} with {:?}", op, new_op);
        Ok(new_op)
    }
}

impl Mutator for InitExpressionMutator {
    fn mutate<'a>(
        self,
        config: &'a mut crate::WasmMutate,
    ) -> crate::Result<Box<dyn Iterator<Item = crate::Result<wasm_encoder::Module>> + 'a>> {
        let new_value = match config.rng().gen::<u8>() {
            0..=99 => 0,
            100..=199 => 1,
            200.. => config.rng().gen::<u128>(),
        };
        let mut translator = InitTranslator {
            new_value,
            replace_for_ty: None,
        };
        match self.0 {
            Item::Global => {
                let num_total = config.info().num_local_globals();
                let mutate_idx = config.rng().gen_range(0..num_total);
                let info = config.info();
                let section = info.globals.ok_or(Error::no_mutations_applicable())?;
                let mut new_section = GlobalSection::new();
                let mut reader = GlobalSectionReader::new(info.raw_sections[section].data, 0)?;
                for idx in 0..reader.get_count() {
                    config.consume_fuel(1)?;
                    let start = reader.original_position();
                    let global = reader.read()?;
                    let end = reader.original_position();
                    if idx == mutate_idx {
                        translator.replace_for_ty = Some(global.ty.content_type);
                        translator.translate_global(global, &mut new_section)?;
                        translator.replace_for_ty = None;
                    } else {
                        new_section.raw(&info.raw_sections[section].data[start..end]);
                    }
                }
                Ok(Box::new(std::iter::once(Ok(
                    info.replace_section(section, &new_section)
                ))))
            }
            Item::Element => {
                let num_total = config.info().num_elements();
                // Select what portion of the element to modify more precisely. We can pick between
                // modifying an offset or one of the values being written to the table.
                let mutate_idx = config.rng().gen_range(0..num_total);
                let info = config.info();
                let section = info.elements.ok_or(Error::no_mutations_applicable())?;
                let mut new_section = ElementSection::new();
                let mut reader = ElementSectionReader::new(info.raw_sections[section].data, 0)?;
                for idx in 0..reader.get_count() {
                    config.consume_fuel(1)?;
                    let start = reader.original_position();
                    let element = reader.read()?;
                    let end = reader.original_position();
                    if idx == mutate_idx {
                        // For now we only modify the offset.
                        translator.translate_element(element, &mut new_section)?;
                    } else {
                        new_section.raw(&info.raw_sections[section].data[start..end]);
                    }
                }
                Ok(Box::new(std::iter::once(Ok(
                    info.replace_section(section, &new_section)
                ))))
            }
            _ => Err(Error::no_mutations_applicable()),
        }

        // let num_globals = reader.get_count();
        // debug_assert_eq!(local_globals, reader.get_count());
        // println!("gunna do it! idx={} to {}", idx_to_mutate, new_value);
        // for idx in 0..num_globals {
        //     config.consume_fuel(1)?;
        //     let start = reader.original_position();
        //     let global = reader.read()?;
        //     let end = reader.original_position();
        //     if idx != idx_to_mutate {
        //         DefaultTranslator.translate_global(global, &mut new_section)?;
        //     } else {
        //         use wasm_encoder::Instruction as I;
        //         use wasmparser::Type as T;
        //         new_section.global(
        //             DefaultTranslator.translate_global_type(&global.ty)?,
        //             &match global.ty.content_type {
        //                 T::I32 => I::I32Const(new_value as _),
        //                 T::I64 => I::I64Const(new_value as _),
        //                 T::F32 => I::F32Const(f32::from_bits(new_value as _)),
        //                 T::F64 => I::F64Const(f64::from_bits(new_value as _)),
        //                 T::V128 => I::V128Const(new_value),
        //                 T::FuncRef => I::RefNull(wasm_encoder::ValType::FuncRef),
        //                 T::ExternRef => I::RefNull(wasm_encoder::ValType::ExternRef),
        //                 T::ExnRef => unimplemented!("wasm encoder does not support this yet"),
        //                 T::Func => todo!(),
        //                 T::EmptyBlockType => todo!(),
        //             },
        //         );
        //     }
        // }
        // Ok(Box::new(std::iter::once(Ok(config
        //     .info()
        //     .replace_section(
        //         config.info().globals.unwrap(),
        //         &new_section,
        //     )))))
    }

    fn can_mutate(&self, config: &crate::WasmMutate) -> bool {
        !config.preserve_semantics
            && match self.0 {
                Item::Global => config.info().num_local_globals() > 0,
                Item::Element => config.info().num_elements() > 0,
                _ => false,
            }
    }
}
//
// #[derive(Copy, Clone)]
// pub struct ElemInitExpressionMutator;
//
// impl Mutator for ElemInitExpressionMutator {
//     fn mutate<'a>(
//         self,
//         config: &'a mut crate::WasmMutate,
//     ) -> crate::Result<Box<dyn Iterator<Item = crate::Result<wasm_encoder::Module>> + 'a>> {
//         let mut new_section = wasm_encoder::ElementSection::new();
//         let local_globals = config.info().num_elements();
//         let idx_to_mutate = config.rng().gen_range(0..local_globals);
//         let new_value = match config.rng().gen::<u8>() {
//             0..=85 => 0,
//             86..=170 => 1,
//             171.. => config.rng().gen::<i128>(),
//         };
//         let mut reader = ElementSectionReader::new(
//             config.info().raw_sections[config.info().elements.unwrap()].data, 0)?;
//         let num_globals = reader.get_count();
//         debug_assert_eq!(local_globals, reader.get_count());
//         println!("gunna do it! idx={} to {}", idx_to_mutate, new_value);
//         for idx in 0..num_globals {
//             config.consume_fuel(1)?;
//             let start = reader.original_position();
//             let element = reader.read()?;
//             let end = reader.original_position();
//             DefaultTranslator.translate_element(element, &mut new_section)?;
// //            if idx != idx_to_mutate {
// //                new_section.raw(&config.info().get_global_section().data[start..end]);
// //            } else {
// //                use wasm_encoder::Instruction as I;
// //                use wasmparser::Type as T;
// //                new_section.segment(
// //                    wasm_encoder::ElementSegment {
// //                        mode: todo!(),
// //                        element_type: todo!(),
// //                        elements: todo!(),
// //                    }
// //                );
// //                // new_section.(
// //                //     wasm_encoder::GlobalType {
// //                //         val_type: translate::ty(NoTranslator.as_obj(), &global.ty.content_type)?,
// //                //         mutable: global.ty.mutable,
// //                //     },
// //                //     &match global.ty.content_type {
// //                //         T::I32 => I::I32Const(new_value as _),
// //                //         T::I64 => I::I64Const(new_value as _),
// //                //         T::F32 => I::F32Const(f32::from_bits(new_value as _)),
// //                //         T::F64 => I::F64Const(f64::from_bits(new_value as _)),
// //                //         T::V128 => I::V128Const(new_value),
// //                //         T::FuncRef => I::RefNull(wasm_encoder::ValType::FuncRef),
// //                //         T::ExternRef => I::RefNull(wasm_encoder::ValType::ExternRef),
// //                //         T::ExnRef => unimplemented!("wasm encoder does not support this yet"),
// //                //         T::Func => todo!(),
// //                //         T::EmptyBlockType => todo!(),
// //                //     },
// //                // );
// //            }
//         }
//         Ok(Box::new(std::iter::once(Ok(config
//             .info()
//             .replace_section(
//                 config.info().globals.unwrap(),
//                 &new_section,
//             )))))
//     }
//
//     fn can_mutate(&self, config: &crate::WasmMutate) -> bool {
//         !config.preserve_semantics && config.info().num_elements() > 0
//     }
// }
