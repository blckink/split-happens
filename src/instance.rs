use crate::app::PartyConfig;
use crate::util::get_screen_resolution;

#[derive(Clone)]
pub struct Instance {
    pub devices: Vec<usize>,
    pub profname: String,
    pub profselection: usize,
    pub width: u32,
    pub height: u32,
}

pub fn set_instance_resolutions(instances: &mut Vec<Instance>, cfg: &PartyConfig) {
    let (basewidth, baseheight) = get_screen_resolution();
    let playercount = instances.len();

    let mut i = 0;
    for instance in instances {
        let (mut w, mut h) = match playercount {
            1 => (basewidth, baseheight),
            2 => {
                if cfg.vertical_two_player {
                    (basewidth / 2, baseheight)
                } else {
                    (basewidth, baseheight / 2)
                }
            }
            _ => (basewidth / 2, baseheight / 2),
        };
        if h < 600 && cfg.gamescope_fix_lowres {
            let ratio = w as f32 / h as f32;
            h = 600;
            w = (h as f32 * ratio) as u32;
        }
        println!("Resolution for instance {}/{playercount}: {w}x{h}", i + 1);
        instance.width = w;
        instance.height = h;
        i += 1;
    }
}

pub fn set_instance_names(instances: &mut Vec<Instance>, profiles: &[String]) {
    // Track how many guest slots have been assigned so we can number them sequentially even
    // when non-guest profiles appear between guest instances.
    let mut next_guest_index = 1usize;

    for instance in instances.iter_mut() {
        // Resolve the selected profile name and gracefully handle stale indices by
        // falling back to a fresh guest slot. This keeps long-standing profiles from being
        // misidentified as guests when the profile picker omits the synthetic "Guest" entry.
        match profiles.get(instance.profselection) {
            Some(selected) if selected == "Guest" => {
                instance.profname = format!("Guest{next_guest_index}");
                next_guest_index += 1;
            }
            Some(selected) => {
                instance.profname = selected.to_owned();
            }
            None => {
                instance.profname = format!("Guest{next_guest_index}");
                next_guest_index += 1;
            }
        }
    }
}
