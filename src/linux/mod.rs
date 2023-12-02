pub mod fdisk;
pub mod luks;
pub mod lvm;
pub mod mkfs;
pub mod mount;
pub mod user;

// See linux/block/partition-generic.c
//
// disk_name() is used by partition check code and the genhd driver.
// It formats the devicename of the indicated disk into
// the supplied buffer (of size at least 32), and returns
// a pointer to that same buffer (for convenience).
//
// char *disk_name(struct gendisk *hd, int partno, char *buf)
// {
// 	if (!partno)
// 		snprintf(buf, BDEVNAME_SIZE, "%s", hd->disk_name);
// 	else if (isdigit(hd->disk_name[strlen(hd->disk_name)-1]))
// 		snprintf(buf, BDEVNAME_SIZE, "%sp%d", hd->disk_name, partno);
// 	else
// 		snprintf(buf, BDEVNAME_SIZE, "%s%d", hd->disk_name, partno);
// 	return buf;
// }
//
pub(crate) fn partition_name(name: &str, part_number: u8) -> String {
    let last_char = name.chars().last().expect("empty name");

    if last_char.is_numeric() {
        return format!("{name}p{part_number}");
    }

    format!("{name}{part_number}")
}
