//! Type 20 current-clock initializer. Type 21/CSV and raw decoding stay separate.

use super::{DecodedRecord, encode_gps_into, reset_holder};
use crate::{
    csv::{date_input::Clock, datetime},
    gps::operations::Record,
    icf::{Error, Icf, context::Context, settings::gps::gps_group_count_source},
    repeater::working::DefaultContext,
};

fn invalid(reason: &'static str) -> Error {
    Error::Invalid { line: 0, reason }
}

/// Native 610C/type 20 reads current 396FF, subtracts that signed-minute offset
/// from GetLocalTime, and only THEN packs calendar/time bytes. No system clock is
/// acquired here and no completed 616-byte holder is accepted from a client.
pub fn holder(
    image: &Icf<'_>,
    q: &Context,
    system_langid: Option<u16>,
    index: u32,
    binding: u32,
    clock: Clock,
) -> Result<DecodedRecord, Error> {
    let mut out = reset_holder(0x20, index, binding);
    let offset = i16::from_le_bytes(
        image
            .memory()
            .get(0x396ff..0x39701)
            .ok_or(invalid("GPS initializer offset source unavailable"))?
            .try_into()
            .unwrap(),
    );
    let offset = datetime::effective_offset(offset)
        .map_err(|_| invalid("GPS initializer offset EncodeTime failed"))?;
    let (date, time) = clock
        .datetime()
        .map_err(|_| invalid("GPS initializer requires valid local clock fields"))?;
    let (date, time) = datetime::shift_minutes(date, time, -offset)
        .map_err(|_| invalid("GPS initializer shifted clock outside calendar"))?;
    let bytes = out.as_mut_bytes();
    bytes[0x254..0x25a].copy_from_slice(&[
        datetime::encode_bcd(date.day),
        datetime::encode_bcd(date.month),
        datetime::encode_bcd((date.year % 100) as u8),
        datetime::encode_bcd(time.hour),
        datetime::encode_bcd(time.minute),
        datetime::encode_bcd(time.second),
    ]);
    bytes[0x25a..0x25d].fill(0);
    bytes[0x25e..0x261].fill(0);
    // The same 10420 Q/source selection is already implemented for type 22.
    let directions = DefaultContext::from_context(q, system_langid)
        .map_err(|_| invalid("GPS initializer Q/source unavailable"))?
        .hemispheres;
    bytes[0x25d] = directions[0];
    bytes[0x261] = directions[1];
    bytes[0x264..0x268].copy_from_slice(&i32::MAX.to_le_bytes());
    // C84A4 accumulates native C8100 counts and compares the LOW WORD index.
    let mut total = 0_u32;
    let mut group = 27;
    for candidate in 0..27_u8 {
        total += u32::from(gps_group_count_source(image.memory(), candidate)?);
        if total > u32::from(index as u16) {
            group = u32::from(candidate);
            break;
        }
    }
    out.set_group(group);
    Ok(out)
}

/// D15A4 allocates metadata FIRST, then 610C(type 20, index 0), then the EXISTING
/// type 0A saver. The caller keeps allocation/notification order and failure prefix.
pub fn seed(
    image: &Icf<'_>,
    q: &Context,
    system_langid: Option<u16>,
    clock: Clock,
) -> Result<Record, Error> {
    let holder = holder(image, q, system_langid, 0, 0, clock)?;
    let mut bytes = [0; 34];
    encode_gps_into(&holder, &mut bytes, |_, _| Ok(()))?;
    Ok(Record {
        valid: holder.valid(),
        bytes,
    })
}
