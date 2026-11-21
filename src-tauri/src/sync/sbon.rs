//! Minimal reader for Starbound's `.player` files (versioned SBON), just
//! enough to pull out the character name without parsing the whole tree.

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn byte(&mut self) -> Option<u8> {
        let v = *self.b.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.i.checked_add(n)?;
        let s = self.b.get(self.i..end)?;
        self.i = end;
        Some(s)
    }
    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for _ in 0..10 {
            let b = self.byte()?;
            v = (v << 7) | (b & 0x7f) as u64;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    fn string(&mut self) -> Option<&'a [u8]> {
        let n = self.varint()? as usize;
        self.take(n)
    }
    /// Skip one value of type `t`.
    fn skip(&mut self, t: u8, depth: u32) -> Option<()> {
        if depth > 64 {
            return None;
        }
        match t {
            1 => {}
            2 => {
                self.take(8)?;
            }
            3 => {
                self.take(1)?;
            }
            4 => {
                self.varint()?;
            }
            5 => {
                self.string()?;
            }
            6 => {
                for _ in 0..self.varint()? {
                    let t = self.byte()?;
                    self.skip(t, depth + 1)?;
                }
            }
            7 => {
                for _ in 0..self.varint()? {
                    self.string()?;
                    let t = self.byte()?;
                    self.skip(t, depth + 1)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
    /// Descend through nested maps along `path` and return the string there.
    fn find(&mut self, path: &[&str]) -> Option<String> {
        let t = self.byte()?;
        let Some((first, rest)) = path.split_first() else {
            return if t == 5 { Some(String::from_utf8_lossy(self.string()?).into_owned()) } else { None };
        };
        if t != 7 {
            return None;
        }
        for _ in 0..self.varint()? {
            let key = self.string()?;
            if key == first.as_bytes() {
                return self.find(rest);
            }
            let vt = self.byte()?;
            self.skip(vt, 0)?;
        }
        None
    }
}

/// The character's display name from a `.player` file's bytes.
pub fn player_name(bytes: &[u8]) -> Option<String> {
    let mut r = Rd { b: bytes, i: 0 };
    if r.take(6)? != b"SBVJ01" {
        return None;
    }
    r.string()?; // entity type, e.g. "PlayerEntity"
    r.byte()?; // has-version flag
    r.take(4)?; // version
    r.find(&["identity", "name"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(out: &mut Vec<u8>, v: &str) {
        out.push(v.len() as u8);
        out.extend(v.as_bytes());
    }

    #[test]
    fn reads_name_skipping_other_values() {
        let mut b = b"SBVJ01".to_vec();
        s(&mut b, "PlayerEntity");
        b.extend([1, 0, 0, 0, 31]);
        b.extend([7, 2]); // map, 2 entries
        s(&mut b, "movement");
        b.extend([6, 2, 4, 6, 3, 1]); // list: varint, bool
        s(&mut b, "identity");
        b.extend([7, 1]);
        s(&mut b, "name");
        b.push(5);
        s(&mut b, "Ada");
        assert_eq!(player_name(&b).as_deref(), Some("Ada"));
    }

    #[test]
    fn garbage_is_none() {
        assert_eq!(player_name(b"nope"), None);
        assert_eq!(player_name(b""), None);
    }

    #[test]
    fn real_file_if_present() {
        let p = "/home/evonz/games/starbound/1.4.4/nicks-old/storage/player/0eadafbae828fbadbcf22e89e2b18515.player";
        if let Ok(b) = std::fs::read(p) {
            let n = player_name(&b);
            eprintln!("name = {n:?}");
            assert!(n.is_some());
        }
    }
}
