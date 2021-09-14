
use rand::{Rng, RngCore, prelude::SmallRng};
use wasm_encoder::{CodeSection, Export, ExportSection, Function, Instruction, RawSection, Section, SectionId, encoders};
use wasmparser::{Chunk, Parser, Payload, Range};
use crate::{ModuleInfo, WasmMutate};


pub trait Mutator
{
    
    /// Method where the mutation happpens
    ///
    /// * `context` instance of WasmMutate
    /// * `chunk` piece of the byte stream corresponding with the payload
    /// * `out_buffer` resulting mutated byte stream
    /// * `mutable` mutable object
    /// Return the number of written bytes
    fn mutate<'a>(&mut self, _:&'a WasmMutate, input_wasm: &[u8], info: &ModuleInfo) -> Vec<u8>;

    /// Returns if this mutator can be applied with the info and the byte range in which it can be applied
    fn can_mutate<'a>(&self, _:&'a WasmMutate, info: &ModuleInfo) -> (bool, Range){
        (false, Range{start:0, end: 0})
    }

    /// Provides the name of the mutator, mostly used for debugging purposes
    fn name(&self) -> String {
        return format!("{:?}", std::any::type_name::<Self>())
    }

}


macro_rules! parse_loop {
    ($chunk: expr, $(($pat: pat, $todo: expr),)*) => {
        let mut parser = Parser::new(0);
        // Hack !
        parser.parse(b"\0asm\x01\0\0\0", false);
        let mut consumed = 0;
        let sectionsize = $chunk.len();

        loop {
            let (payload, chunksize) = match parser.parse(&$chunk[consumed..], false).unwrap() {
                Chunk::NeedMoreData(_) => {
                    panic!("Invalid Wasm module");
                },
                Chunk::Parsed { consumed, payload } => (payload, consumed),
            };

            match payload {
                $(
                    $pat => { $todo },
                )*
                _ => panic!("This mutator cannot be applied to this section")
            }
            
            consumed += chunksize;

            if consumed == sectionsize {
                break
            }
        }
    };
}

// Concrete implementations
pub struct ReturnI32SnipMutator;

impl Mutator for ReturnI32SnipMutator {
    fn mutate<'a>(&mut self, _:&'a WasmMutate, input_wasm: &[u8], info: &ModuleInfo) -> Vec<u8>
    {
        let mut function_count = 0;
        let mut codes = Vec::new();
        
        parse_loop!{
           &input_wasm[info.code.start..info.code.end], 
           (Payload::CodeSectionEntry(_), {
                let locals = vec![];                    
                let mut tmpbuff: Vec<u8> = Vec::new();
                let mut f = Function::new(locals);
                f.instruction(Instruction::I32Const(0));
                f.instruction(Instruction::End);
                f.encode(&mut tmpbuff);
                codes.extend(tmpbuff);
           }),
           (Payload::CodeSectionStart{count, range, size}, {
                function_count = count;
            }),
        };

        let mut result = Vec::new();
        let mut sink = Vec::new();
        let num_added = encoders::u32(function_count);
        sink.extend(num_added);
        sink.extend(codes.iter().copied());

        let raw_code = RawSection{
            id: SectionId::Code.into(),
            data: &sink
        };

        // To check, add section if in the encoding of the section ?
        result.extend(std::iter::once(SectionId::Code as u8));
        raw_code.encode(&mut result);
        result
    }

    fn can_mutate<'a>(&self, _:&'a WasmMutate, info: &ModuleInfo) -> (bool, Range) {
        let code = info.code;
        (info.has_code(), code)
    }
}



pub struct SetFunction2Unreachable;

impl Mutator for SetFunction2Unreachable{
    fn mutate<'a>(&mut self, _:&'a WasmMutate, input_wasm: &[u8], info: &ModuleInfo) -> Vec<u8>
    {
        let mut function_count = 0;
        let mut codes = Vec::new();
        
        parse_loop!{
           &input_wasm[info.code.start..info.code.end], 
           (Payload::CodeSectionEntry(_), {
                
                let locals = vec![];                    
                let mut tmpbuff: Vec<u8> = Vec::new();
                let mut f = Function::new(locals);
                f.instruction(Instruction::Unreachable);
                f.instruction(Instruction::End);
                f.encode(&mut tmpbuff);

                codes.extend(tmpbuff);
           }),
           (Payload::CodeSectionStart{count, range, size}, {
                function_count = count;
            }),
        };

        let mut result = Vec::new();
        let mut sink = Vec::new();
        let num_added = encoders::u32(function_count);
        sink.extend(num_added);
        sink.extend(codes.iter().copied());

        let raw_code = RawSection{
            id: SectionId::Code.into(),
            data: &sink
        };

        // To check, add section if in the encoding of the section ?
        result.extend(std::iter::once(SectionId::Code as u8));
        raw_code.encode(&mut result);
        result
    }
    
    fn can_mutate<'a>(&self, _:&'a WasmMutate, info: &ModuleInfo) -> (bool, Range) {
        let code = info.code;
        (info.has_code(),code)
    }
}



pub struct RemoveExportMutator ;

impl Mutator for RemoveExportMutator{
    fn mutate<'a>(&mut self, config:&'a WasmMutate, input_wasm: &[u8], info: &ModuleInfo) -> Vec<u8>
    {
        let mut sink = Vec::new();

        parse_loop!{
            &input_wasm[info.exports.start..info.exports.end], 
            (Payload::ExportSection(mut reader), {
                let mut exports = ExportSection::new();
                let max_exports = reader.get_count() as u64;
                let skip_at = config.get_rnd().gen_range(0, max_exports);

                (0..max_exports).for_each(|i|{ 
                    let export = reader.read().unwrap();
                    if skip_at != i { // otherwise bypass
                        match export.kind {
                            wasmparser::ExternalKind::Function => { exports.export(export.field, Export::Function(export.index)); },
                            wasmparser::ExternalKind::Table => { exports.export(export.field, Export::Table(export.index)); },
                            wasmparser::ExternalKind::Memory => { exports.export(export.field, Export::Memory(export.index)); },
                            wasmparser::ExternalKind::Global => { exports.export(export.field, Export::Global(export.index)); },
                            wasmparser::ExternalKind::Module => { exports.export(export.field, Export::Module(export.index)); },
                            wasmparser::ExternalKind::Instance => { exports.export(export.field, Export::Instance(export.index)); },
                            _ => {
                                panic!("Unknown export {:?}", export)
                            }
                        }
                    } else {
                        #[cfg(debug_assertions)] {
                            eprintln!("Removing export {:?} idx {:?}", export, skip_at);
                        }
                    }
                });
                sink.extend(std::iter::once(SectionId::Export as u8));
                let mut tmpbuf = Vec::new();
                exports.encode(&mut tmpbuf);
                sink.extend(tmpbuf);
            }),
         };
         sink
    }

    
    fn can_mutate<'a>(&self, _:&'a WasmMutate, info: &ModuleInfo) -> (bool, Range) {
        let exports = info.exports;
        (info.has_exports(), exports)
    }
}


pub struct RenameExportMutator {
    pub max_name_size: u32
}

impl RenameExportMutator {
    
    // Copied and transformed from wasm-smith name generation
    fn limited_string(&self, rnd: &mut SmallRng) -> String {
        
        let size = rnd.gen_range(1, self.max_name_size);
        let size = std::cmp::min(size, self.max_name_size);
        let mut str = vec![0u8; size as usize];
        rnd.fill_bytes(&mut str);

        match std::str::from_utf8(&str) {
            Ok(s) => {
                String::from(s)
            }
            Err(e) => {
                let i = e.valid_up_to();
                let valid = &str[0..i];
                let s = unsafe {
                    debug_assert!(std::str::from_utf8(valid).is_ok());
                    std::str::from_utf8_unchecked(valid)
                };
                String::from(s)
            }
        }
    }

}

impl Mutator for RenameExportMutator{

    fn mutate<'a>(&mut self, config:&'a WasmMutate, input_wasm: &[u8], info: &ModuleInfo) -> Vec<u8>
    {


        let mut sink = Vec::new();

        parse_loop!{
            &input_wasm[info.exports.start..info.exports.end], 
            (Payload::ExportSection(mut reader), {
                
                println!("Applying RemoveExportMutator");
                let mut exports = ExportSection::new();
                let max_exports = reader.get_count() as u64;
                let skip_at = config.get_rnd().gen_range(0, max_exports);

                (0..max_exports).for_each(|i|{ 
                    let export = reader.read().unwrap();

                    let new_name = if skip_at != i { // otherwise bypass
                        String::from(export.field)
                    } else {
                        let new_name = self.limited_string(&mut config.get_rnd());
                        #[cfg(debug_assertions)] {
                            eprintln!("Renaming export {:?} by {:?}", export, new_name);
                        }
                        new_name
                    };

                    if skip_at != i { // otherwise bypass
                        match export.kind {
                            wasmparser::ExternalKind::Function => { exports.export(new_name.as_str(), Export::Function(export.index)); },
                            wasmparser::ExternalKind::Table => { exports.export(new_name.as_str(), Export::Table(export.index)); },
                            wasmparser::ExternalKind::Memory => { exports.export(new_name.as_str(), Export::Memory(export.index)); },
                            wasmparser::ExternalKind::Global => { exports.export(new_name.as_str(), Export::Global(export.index)); },
                            wasmparser::ExternalKind::Module => { exports.export(new_name.as_str(), Export::Module(export.index)); },
                            wasmparser::ExternalKind::Instance => { exports.export(new_name.as_str(), Export::Instance(export.index)); },
                            _ => {
                                panic!("Unknown export {:?}", export)
                            }
                        }
                    } else {
                        #[cfg(debug_assertions)] {
                            eprintln!("Removing export {:?} idx {:?}", export, skip_at);
                        }
                    }
                });
                sink.extend(std::iter::once(SectionId::Export as u8));
                let mut tmpbuf = Vec::new();
                exports.encode(&mut tmpbuf);
                sink.extend(tmpbuf);
            }),
         };
         sink
    }

    
    fn can_mutate<'a>(&self, _:&'a WasmMutate, info: &ModuleInfo) -> (bool, Range) {
        let exports = info.exports;
        (info.has_exports(), exports)
    }
    
}