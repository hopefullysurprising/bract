use std::path::Path;

use goblin::Object;

const BUILDINFO_MAGIC: &[u8] = b"\xff Go buildinf:";
const FLAGS_VERSION_INL: u8 = 0x2;

pub struct GoDep {
    pub path: String,
    pub version: String,
}

pub fn read_deps(binary_path: &Path) -> Option<Vec<GoDep>> {
    let data = std::fs::read(binary_path).ok()?;
    let section_data = find_buildinfo_section(&data)?;
    parse_buildinfo(section_data)
}

fn find_buildinfo_section(data: &[u8]) -> Option<&[u8]> {
    let object = Object::parse(data).ok()?;
    match object {
        Object::Mach(goblin::mach::Mach::Binary(mach)) => {
            for segment in &mach.segments {
                for (section, _) in segment.sections().ok()? {
                    if section.name().ok() == Some("__go_buildinfo") {
                        let offset = section.offset as usize;
                        let size = section.size as usize;
                        return data.get(offset..offset + size);
                    }
                }
            }
            None
        }
        Object::Elf(elf) => {
            for section in &elf.section_headers {
                let name = elf.shdr_strtab.get_at(section.sh_name)?;
                if name == ".go.buildinfo" {
                    let offset = section.sh_offset as usize;
                    let size = section.sh_size as usize;
                    return data.get(offset..offset + size);
                }
            }
            None
        }
        Object::PE(pe) => {
            for section in &pe.sections {
                let name = section.name().ok()?;
                if name == ".go.buildinfo" {
                    let offset = section.pointer_to_raw_data as usize;
                    let size = section.size_of_raw_data as usize;
                    return data.get(offset..offset + size);
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_buildinfo(section: &[u8]) -> Option<Vec<GoDep>> {
    let magic_offset = find_aligned_magic(section)?;
    let header = section.get(magic_offset..magic_offset + 32)?;

    let flags = header[15];
    if flags & FLAGS_VERSION_INL == 0 {
        return None;
    }

    let mut pos = magic_offset + 32;
    let (_version, bytes_read) = read_varint_bytes(section, pos)?;
    pos += bytes_read;
    let (mod_info_bytes, _) = read_varint_bytes(section, pos)?;

    parse_mod_info(mod_info_bytes)
}

fn find_aligned_magic(data: &[u8]) -> Option<usize> {
    let mut offset = 0;
    while offset + 32 <= data.len() {
        let aligned = (offset + 15) & !15;
        if aligned + 14 > data.len() {
            break;
        }
        if data.get(aligned..aligned + 14)? == BUILDINFO_MAGIC {
            return Some(aligned);
        }
        offset = aligned + 16;
    }
    None
}

fn read_varint_bytes<'a>(data: &'a [u8], start: usize) -> Option<(&'a [u8], usize)> {
    let mut value: u64 = 0;
    let mut shift = 0;
    let mut pos = start;

    loop {
        let byte = *data.get(pos)?;
        pos += 1;
        value |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }

    let len = value as usize;
    let bytes = data.get(pos..pos + len)?;
    Some((bytes, pos - start + len))
}

fn parse_mod_info(raw: &[u8]) -> Option<Vec<GoDep>> {
    if raw.len() < 33 || raw[raw.len() - 17] != b'\n' {
        return None;
    }
    let content = std::str::from_utf8(&raw[16..raw.len() - 16]).ok()?;
    let deps = content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first()? != &"dep" || parts.len() < 3 {
                return None;
            }
            Some(GoDep {
                path: parts[1].to_string(),
                version: parts[2].to_string(),
            })
        })
        .collect();
    Some(deps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mise_install_path(tool: &str) -> PathBuf {
        let home = std::env::var("HOME").unwrap();
        PathBuf::from(home).join(".local/share/mise/installs").join(tool)
    }

    fn has_dep(deps: &[GoDep], path: &str) -> bool {
        deps.iter().any(|d| d.path == path)
    }

    #[test]
    fn detects_cobra_in_devspace() {
        let path = mise_install_path("aqua-devspace-sh-devspace/6.3.18/devspace");
        if !path.exists() {
            return;
        }
        let deps = read_deps(&path).expect("should parse buildinfo");
        assert!(has_dep(&deps, "github.com/spf13/cobra"));
    }

    #[test]
    fn detects_cobra_in_atlas() {
        let path = mise_install_path("aqua-ariga-atlas/1.0.0/atlas");
        if !path.exists() {
            return;
        }
        let deps = read_deps(&path).expect("should parse buildinfo");
        assert!(has_dep(&deps, "github.com/spf13/cobra"));
    }

    #[test]
    fn detects_urfave_not_cobra_in_step() {
        let path = mise_install_path("aqua-smallstep-cli/0.28.6/step_darwin_arm64/bin/step");
        if !path.exists() {
            return;
        }
        let deps = read_deps(&path).expect("should parse buildinfo");
        assert!(has_dep(&deps, "github.com/urfave/cli"));
        assert!(!has_dep(&deps, "github.com/spf13/cobra"));
    }

    #[test]
    fn returns_none_for_non_go_binary() {
        let path = mise_install_path("aqua-jqlang-jq/1.8.1/jq");
        if !path.exists() {
            return;
        }
        let result = read_deps(&path);
        assert!(result.is_none());
    }
}
