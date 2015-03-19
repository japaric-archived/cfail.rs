//! Matching annotations and messages

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BitVec;

use {KINDS, NKINDS, Annotations, Kind, Line, LineMap, Messages};

/// Mismatches for every compiler message kind
#[derive(Debug)]
pub struct Mismatches<'a>([Option<Vec<(Line, Mismatch<'a>)>>; NKINDS]);

impl<'a> Mismatches<'a> {
    fn new() -> Mismatches<'a> {
        Mismatches([None, None, None, None])
    }

    /// Returns the mismatches for this kind of compiler message, if any
    pub fn get(&self, kind: Kind) -> Option<&[(Line, Mismatch<'a>)]> {
        self.0[kind as usize].as_ref().map(|v| &v[..])
    }

    fn insert(&mut self, kind: Kind, line: Line, mismatch: Mismatch<'a>) {
        if let Some(ref mut mismatches) = self.0[kind as usize] {
            mismatches.push((line, mismatch))
        } else {
            self.0[kind as usize] = Some(vec![(line, mismatch)])
        }
    }

    fn push_anns(&mut self, (line, mut anns): (Line, Annotations<'a>)) {
        for &kind in &KINDS {
            if let Some(anns) = anns.take(kind) {
                let mismatch = Mismatch { annotations: anns, messages: vec![] };
                self.insert(kind, line, mismatch)
            }
        }
    }

    fn push_msgs(&mut self, (line, mut msgs): (Line, Messages<'a>)) {
        for &kind in &KINDS {
            if let Some(msgs) = msgs.take(kind) {
                let mismatch = Mismatch { annotations: vec![], messages: msgs };
                self.insert(kind, line, mismatch)
            }
        }
    }
}

/// Mismatches per line
#[derive(Debug)]
pub struct Mismatch<'a> {
    annotations: Vec<Cow<'a, str>>,
    messages: Vec<&'a str>,
}

/// Finds the mismatches between the `cfail` annotations and the compiler messages
pub fn match_<'a>(anns: LineMap<Annotations<'a>>, msgs: LineMap<Messages<'a>>) -> Mismatches<'a> {
    let mut mismatches = Mismatches::new();

    let mut anns = anns.into_iter().peekable();
    let mut msgs = msgs.into_iter().peekable();

    loop {
        match (anns.peek(), msgs.peek()) {
            (None, None) => break,
            (None, Some(_)) => {
                mismatches.push_msgs(msgs.next().unwrap());
            },
            (Some(&(a_ln, _)), Some(&(m_ln, _))) => match a_ln.cmp(&m_ln) {
                Ordering::Equal => {
                    let (line, mut anns) = anns.next().unwrap();
                    let (_, mut msgs) = msgs.next().unwrap();

                    for &kind in &KINDS {
                        if let Some(mismatch) = compare_opt(anns.take(kind), msgs.take(kind)) {
                            mismatches.insert(kind, line, mismatch)
                        }
                    }
                },
                Ordering::Greater => {
                    mismatches.push_msgs(msgs.next().unwrap());
                },
                Ordering::Less => {
                    mismatches.push_anns(anns.next().unwrap());
                },
            },
            (Some(_), None) => {
                mismatches.push_anns(anns.next().unwrap());
            },
        }
    }

    mismatches
}

fn compare_opt<'a>(
    anns: Option<Vec<Cow<'a, str>>>,
    msgs: Option<Vec<&'a str>>,
) -> Option<Mismatch<'a>> {
    match (anns, msgs) {
        (None, None) => None,
        (Some(anns), None) => {
            Some(Mismatch {
                annotations: anns,
                messages: vec![],
            })
        },
        (None, Some(msgs)) => {
            Some(Mismatch {
                annotations: vec![],
                messages: msgs,
            })
        },
        (Some(anns), Some(msgs)) => {
            compare(anns, msgs)
        },
    }
}

fn compare<'a>(anns: Vec<Cow<'a, str>>, msgs: Vec<&'a str>) -> Option<Mismatch<'a>> {
    let mut matched_anns = BitVec::from_elem(anns.len(), false);
    let mut matched_msgs = BitVec::from_elem(msgs.len(), false);

    for (i, ann) in anns.iter().enumerate() {
        for (j, &msg) in msgs.iter().enumerate() {
            if !matched_anns[i] && !matched_msgs[j] && is_substring(ann, msg) {
                matched_anns.set(i, true);
                matched_msgs.set(j, true);
            }
        }
    }

    if matched_anns.all() && matched_msgs.all() {
        None
    } else {
        Some(Mismatch {
            annotations: anns.into_iter().enumerate().filter_map(|(i, ann)| {
                if !matched_anns[i] {
                    Some(ann)
                } else {
                    None
                }
            }).collect(),
            messages: msgs.into_iter().enumerate().filter_map(|(j, msg)| {
                if !matched_msgs[j] {
                    Some(msg)
                } else {
                    None
                }
            }).collect(),
        })
    }
}

/// Formats all the mismatches
pub fn format(mismatches: Mismatches) -> String {
    let mut buffer = String::new();

    for &kind in &KINDS {
        if let Some(mismatches) = mismatches.get(kind) {
            for &(line, ref mismatched) in mismatches {
                if mismatched.annotations.is_empty() {
                    buffer.push_str(&format!("{}: unmatched {} messages\n", line.0, kind));

                    for msg in &mismatched.messages {
                        buffer.push_str(&format!(" {:?}\n", msg))
                    }
                } else if mismatched.messages.is_empty() {
                    buffer.push_str(&format!("{}: unmatched {} annotations\n", line.0, kind));

                    for ann in &mismatched.annotations {
                        buffer.push_str(&format!(" {:?}\n", ann))
                    }
                } else {
                    buffer.push_str(&format!("{}: mismatched {} annotations\n", line.0, kind));

                    for ann in &mismatched.annotations {
                        buffer.push_str(&format!(" expected: {:?}\n", ann))
                    }

                    for msg in &mismatched.messages {
                        buffer.push_str(&format!("    found: {:?}\n", msg))
                    }
                }
            }
        }
    }

    buffer
}

/// Is the annotation a substring of the compiler message?
fn is_substring(ann: &str, msg: &str) -> bool {
    let mut ann_lines = ann.lines().peekable();

    for msg_line in msg.lines() {
        match ann_lines.peek() {
            Some(&ann_line) => {
                if msg_line.contains(ann_line) {
                    ann_lines.next();
                }
            },
            None => return true,
        }
    }

    ann_lines.count() == 0
}

#[cfg(test)]
mod test {
    #[test]
    fn is_substring() {
        let ann = "does not implement";
        let msg = "type `_` does not implement any method in scope named `count_zeros`";

        assert!(super::is_substring(ann, msg));
    }

    #[test]
    fn is_substring_multiline() {
        let ann = "mismatched types\nexpected `i8`\nfound `u8`";
        let msg = "mismatched types:\n expected `i8`,\n    found `u8`\n(expected i8,\
                   \n    found u8) [E0308]";
        assert!(super::is_substring(ann, msg));
    }
}
