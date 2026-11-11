# CS-705 Reverse Engineering Notes

This document describes the settings image, record codecs, document operations, file formats, and clone protocol of the Windows application `CS705.exe`, as reconstructed from its machine code.

- **Executable:** `CS705.exe`, CS-705 1.30
- **Architecture:** PE32, i386
- **SHA-256:** `e9439e1413e1258be77e6ef2188b72276dd714642e3858f4690e5d45653aeca3`
- **Image base:** `0x400000`
- **Current layout:** MapRev 6
- **Internal image length, `M`:** `0x4997A` bytes
- **Current-layout seed prefix and file payload length, `N`:** `0x3C9C0` bytes

Addresses and offsets use hexadecimal with a `0x` prefix; byte sequences use hexadecimal pairs. Counts, record sizes, indices, and physical quantities are decimal unless marked otherwise. Memory windows use half-open notation `[start,end)`; inclusive ranges are identified explicitly. `u32(x)` reduces `x` modulo `2^32`; subsequent division is unsigned integer division. Image DWORDs and words are little-endian unless marked BE.

`RVA` is relative to the executable image base, with `VA = RVA + 0x400000`. `I[offset]` addresses a settings-image byte; `D`, `H`, `Q`, and `B` denote the document, decoded record holder, capability context, and range owner. These symbols and the image-role names below are names used in this document.

### Table of Contents

1. [Image Layout](#1-image-layout)
   - [Document State and Image Roles](#11-document-state-and-image-roles)
   - [Image Allocation](#12-image-allocation)
2. [Memory Addressing and Row Operations](#2-memory-addressing-and-row-operations)
   - [Logical Rows and Physical Slots](#21-logical-rows-and-physical-slots)
   - [Allocation and Row Operations](#22-allocation-and-row-operations)
   - [Metadata Reconstruction](#23-metadata-reconstruction)
3. [Records and Editing](#3-records-and-editing)
   - [Normal and Call Records](#31-normal-and-call-records)
   - [Encoding and Reload](#32-encoding-and-reload)
   - [Editing State](#33-editing-state)
   - [Other Memory Families](#34-other-memory-families)
4. [Capability and Default Generation](#4-capability-and-default-generation)
   - [Capability Context](#41-capability-context)
   - [Generation Sequence](#42-generation-sequence)
   - [User-Edge Generation](#43-user-edge-generation)
   - [Document Normalization](#44-document-normalization)
5. [Document Operations](#5-document-operations)
   - [Dirty State and Pending Save](#51-dirty-state-and-pending-save)
   - [New and Open](#52-new-and-open)
   - [Save](#53-save)
   - [Target Selection](#54-target-selection)
6. [MapRev Conversion](#6-maprev-conversion)
   - [Conversion Steps](#61-conversion-steps)
   - [Marker Placement and Object Copy](#62-marker-placement-and-object-copy)
7. [ICF Format](#7-icf-format)
   - [File Structure](#71-file-structure)
   - [Reader Grammar and Metadata](#72-reader-grammar-and-metadata)
   - [CD Generation](#73-cd-generation)
   - [Coverage](#74-coverage)
8. [Clone Protocol](#8-clone-protocol)
   - [Frames and Commands](#81-frames-and-commands)
   - [Data Encoding](#82-data-encoding)
   - [Identification and Profiles](#83-identification-and-profiles)
   - [Controller and Port Output](#84-controller-and-port-output)
   - [Receive State and Coverage](#85-receive-state-and-coverage)
9. [CSV Format and Conversion](#9-csv-format-and-conversion)
   - [Lines, Cells, and Headers](#91-lines-cells-and-headers)
   - [Values and Text](#92-values-and-text)
   - [Row Schemas](#93-row-schemas)
   - [Import, Export, and Cancellation](#94-import-export-and-cancellation)

## 1. Image Layout

Field dispatch maps resource keys and logical selections to typed operations. A holder decodes one record for a type, index, and side; its writer updates shared fields and active overlays. The document owns the images, path, target layout, dirty flag, and I/O-time normalization.

```text
UI field key / logical selection
    -> document state
    -> decoded holder: type, index, valid, side, mode, values
    -> image: records, links, masks, metadata, working tail
    -> ICF / CSV / clone transfer
```

### 1.1 Document State and Image Roles

| Member or Symbol | Location or Source | Contents |
| --- | --- | --- |
| Directory | `D+0x84` | Last directory, updated by the individual file operation |
| Path | `D+0x88` | Document filename |
| Dirty | `D+0x8C` | Sticky modified flag |
| Target | `D+0x100` | Raw save-layout selector |
| `current` | Object at `D+0x13C`, raw pointer at `D+0x140` | Editable image |
| `G` | Object at `D+0x144`, raw pointer at `D+0x148` | Generated default image |
| `baseline` | Object at `D+0x14C` | Image used for dirty comparison |
| `W` | Explicit normalization argument | Image being modified |
| `F` | Pointer captured from `D+0x148` | Default source read during normalization |
| `T` | Raw pointer at `VA 0x8648C4` | Cached current-image pointer |
| Receive image | Transfer result object | Image borrowed by the final read callback |

When `W == F`, writes to the normalization target change subsequent default-source reads. Some helpers also read current through `T` while processing another image. Capability, range containers, generated defaults, and view state are refreshed by separate operations.

### 1.2 Image Allocation

Images occupy `M` bytes. The first `N` bytes hold the current-layout file payload; `[N,M)` contains working state, including logical memory maps and counts. Seed loading copies only the first `N` bytes and retains the working tail. The image reset helper zeroes the entire destination, including when resetting it to its existing length.

Ordinary ICF open starts with a zeroed `0x49E3A`-byte reader image. After reader, signature, and coverage checks, it snapshots that image, resets the destination to `M`, copies the first `M` bytes back, and passes it to the converter.

## 2. Memory Addressing and Row Operations

### 2.1 Logical Rows and Physical Slots

Normal memory has 100 groups of 100 logical rows and a shared pool of 500 physical records. An empty row has no slot; a split row stores A and B in one 83-byte record. Group order is represented by a descriptor chain, and occupied rows within each group by slot links and row masks.

| Structure | MapRev 6 Location | Layout |
| --- | --- | --- |
| Physical records | `I + 83*p` | 500 records, `0 <= p < 500` |
| Group descriptors | `I + 0x355C0 + 19*d` | Next descriptor byte, LE16 head slot, 16-byte name |
| Slot links | `I + 0x35D2C + 2*p` | LE16 links shared by occupied and free chains |
| Logical row masks | `I + 0x36114 + 13*g` | 100 row bits per group; bit 1 means empty |
| Descriptor root | `I + 0x36628` | Descriptor index; `0xFF` terminates the chain |
| Physical slot flags | `I + 0x3662D + p` | Bit 0: free; bits 6–7: SEL |
| Per-group Home | `I + 0x36821 + 2*g` | 100 LE16 values |
| Displayed group extent | `I + 0x3FB40` | DWORD |
| Working group counts | `I + 0x3FB44 + 4*g` | Occupied-row count |
| Working logical map | `I + 0x3FCD4 + 400*g + 4*r` | DWORD slot index or `0xFFFF` |

The loader follows descriptor and occupied-slot chains, consuming one slot for each zero row-mask bit. It retains intervening empty groups and rows, and omits trailing groups whose head is empty and name is all spaces. Named empty groups remain visible. Metadata reconstruction preserves the four unused high bits of each 13-byte row mask.

For normal memory type `0x02`, GUI side-row index `q` maps to:

```text
g = q / 200
r = (q % 200) / 2
s = q & 1
p = working_map[g,r]
record = I + 83*p
side   = record + 1 + 33*s
```

For example, `q=203` selects group 1, row 1, side B. With physical slot 7, the record starts at `I+0x245` and side B at `I+0x267`. Call memory type `0x03` uses four records at `I+0xA21C`, row `(q % 8)/2`, and internal group 100.

### 2.2 Allocation and Row Operations

Allocation scans slot flags from 0 through 499 and takes the lowest free slot. Assignment stores the slot in the working map, clears its free bit, and increments the group count, retaining the existing record payload. Exhaustion stops allocation with earlier assignments intact.

| Operation | Map and Flag Updates | Record Payload |
| --- | --- | --- |
| Clear | Free occupied slots, write `0xFFFF` map entries, decrement count | Retained |
| Delete | Clear selected rows, shift following entries left, fill the end with `0xFFFF` | Surviving slots stay in place |
| Blank insert | Clear displaced tail rows, shift entries right, insert `0xFFFF` entries | Occupied tail rows are discarded; no new slot allocation |
| Paired normal-row move | Rotate map entries by one adjacent position | Slots stay in place |
| Single-side normal or call move | Copy and paste decoded holders | Re-encoded by holder writers |

Deleting one selected side of a split normal row loads the opposite side, clears split, and saves it to the selected side. The row retains its map entry, slot, and occupied count.

Home reset copies `G[0x36821+2*g]` to the working image's group Home word. If global Home group `I[0x3C84B]` equals `g`, it also copies that restored word to global Home row `I[0x3C84C]` and sets an external cache byte. Clear performs the reset when it removes an occupied row; move and insert perform it afterward. Deleting an empty row shifts logical positions without that reset.

### 2.3 Metadata Reconstruction

Save-time reconstruction takes the working map as input, orders descriptors `0..99`, links occupied slots in logical row order, links free slots in ascending physical order, and updates row masks. Physical record bytes remain in place. The resulting descriptor order replaces any original permutation.

The free-chain loop runs `500 - sum(group_counts)` iterations. With stale counts or orphan occupied flags, it can consume the `0xFFFF` sentinel as an index and write near `I+0x35D2A`, overlapping the final descriptor name.

## 3. Records and Editing

### 3.1 Normal and Call Records

```text
Record Offset   Size   Field
0x00              1   Header; bit 1 = split
0x01             33   Side A
0x22             33   Side B
0x43             16   Shared name
0x53                  End
```

Slot occupancy and SEL reside in the separate slot-flag array. Both sides share the name and split bit. The holder contains decoded validity, side, split, name, frequency, mode, DATA, filter, tone, offset, and callsign fields.

| Side Window | Encoding and Write Rule |
| --- | --- |
| `[0x00,0x04)` | Frequency in Hz, LE32, stored directly |
| `0x04` | High nibble: low nibble of mode lookup result; low nibble: filter |
| `0x05` | DATA low bit and two DUP bits; upper five bits retained |
| `[0x06,0x0C)` | Tone, tone squelch, DTCS, and polarity for modes `0x0D`/`0x0E` |
| `[0x06,0x08)` | Digital squelch/code for mode `0x10`; retained for `0x11` |
| `[0x08,0x0F)`, `[0x0F,0x16)`, `[0x16,0x1D)` | Packed seven-byte UR, RPT1, RPT2 for modes `0x10`/`0x11` |
| `[0x1D,0x21)` | Offset in Hz, LE32, clamped to unsigned `0..9999900` |

The normal writer first sets the slot's free bit to the inverse of holder validity. Invalid holders stop there. Invalid call holders skip the record. Valid holders write name, SEL, split, selected-side scalar fields, and mode-dependent overlays, retaining the other side, inactive overlays, and unrelated bits.

Synthetic record fragments, with offsets relative to the record:

```text
0x01  60 27 AC 08                                      Side A: 145500000 Hz
0x22  C0 92 ED 19                                      Side B: 435000000 Hz
0x43  4C 4F 43 41 4C 20 20 20 20 20 20 20 20 20 20 20  Name: "LOCAL"
```

### 3.2 Encoding and Reload

The holder entries at `RVA 0x77B0`, `0x81E8`, and `0x81D0` perform write, write-and-reload, and reload respectively. Mode `0x11` decodes raw DUP 1/2 as holder DUP 3/4; encoding maps 3/4 back to 1/2.

For digital callsign writes, an RPT2 value other than exactly eight spaces triggers a second decode of the saved RPT1, replacement of a trailing `G` with space, and repacking. This decode also strips high bits from the packed value.

Editors and normalization round offsets with `u32(x+50)/100*100`; the raw writer then clamps. Examples: 149→100, 150→200, `0xFFFFFFFF`→0.

### 3.3 Editing State

An empty normal row keeps an entered name in a pending document slot. Registration allocates a physical slot, initializes the holder, copies a nonempty pending name, clears the pending slot, and invokes the writer. Later failure leaves the earlier allocation or pending-name consumption in effect.

Mode, DATA, and filter editors use a cache keyed by `(type,q)`, containing remembered filter state and values from the last generated `G`. Logical row moves invalidate or reload this cache. Split editing compares the full candidate DWORD to select notifications, then stores its low byte and ultimately header bit 1; candidate `0x100` takes a nonzero notification branch while storing a zero split bit.

Text sanitation can modify shared string storage before the editor returns false. Keyer processing scans the other seven rows for star markers, removes competing stars from the candidate, and saves the corrected current row. Display processing can invoke this repair with notification and dirty-comparison flags zero.

### 3.4 Other Memory Families

| Family | Layout | Encoding and Initialization |
| --- | --- | --- |
| Program scan | 25 × 27 bytes at `I+0xA368`; invalid bitmap at `I+0xA60B` | Two frequency DWORDs, whole-byte mode lookup, DATA, filter, name16; bitmap bit 1 means invalid |
| User band edges | 30 × 8 bytes at `I+0x378C0` | Low/high frequency DWORDs |
| Band stacking | 36-byte records; displayed rows 0–44 | Frequency is written before the validity test |
| Memopad | 36-byte records; displayed slots 1–10, reserved slot 0 | Frequency `0xFFFFFFFF` marks a hole; frequency is written before the validity test |

First normal-memory registration combines a generated template with compiled frequency sources. First scan registration uses instruction constants: mode `0x19`, DATA 0, filter `0xFF`, and name filled with spaces. Clearing an existing scan row decodes its generated default, clears validity, and saves the resulting holder.

## 4. Capability and Default Generation

### 4.1 Capability Context

The 24-byte context `Q` resides at `VA 0x8646B0`. Kind and classification are DWORDs; other semantic fields are bytes. Registry `Etc Data` contains a packed representation:

- `[0x00,0x04)`: Kind, bits 18–22.
- `0x04`, `0x05`: Bits 25, 24.
- `[0x08,0x0C)`: Classification, bits 16–17.
- `0x0C`, `0x0D`, `0x0E`, `0x0F`: Bits 8, 7, 29, 26.
- `0x10`, `0x11`, `0x12`, `0x13`: Bits 27, 15, 14, 11.
- `0x16`: Override, retained by external capability adoption.

The packed checksum stores a three-bit value in bits 0, 3, and 6, equal to the population count of the other 29 bits modulo 8. Decoding clears all 24 context bytes and writes the fields before testing the checksum. On failure, the registry caller sets kind to `0x10` and retains the other decoded fields.

The effective-kind getter applies these rules in order:

1. Signed kind at least 16 yields 16.
2. Nonzero override yields 1.
3. Otherwise, kind 0 or 6–15 yields 0; kind 1 yields 1; kind 2/3 yields 2; kind 4 yields 3; kind 5 yields 4.

The third path indexes a compiled table without a lower-bound check. Effective values below 5 select the seed directly. Larger values select seed 0 when `GetSystemDefaultLangID() & 0x3FF` equals `0x11`, and seed 1 otherwise.

### 4.2 Generation Sequence

`B` is the lazy range owner at `VA 0x864450`. Its getter returns the existing owner; explicit generation rebuilds its nine containers. Default generation proceeds as follows:

```text
Adopt Q -> capture seed selector -> rebuild B containers
    -> copy seed prefix N into G -> patch records and scalars
    -> generate user edges -> load working metadata -> normalize G
```

The seed copy retains `G[N,M)`. Subsequent helpers reread live capability and language state. Container rebuilding is sequential, so an exception leaves already-rebuilt containers alongside the remaining old ones.

Range queries use several search rules:

| Entry RVA | Container | Result |
| --- | --- | --- |
| `0xF014` | `B+0x08` | Search backward for the first `low <= frequency`; return index or -1 |
| `0xED70` | `B+0xA8` | Search forward for the first `high >= frequency`; return index or count |
| `0xEECC` | `B+0x120` | Same upper-bound search |
| `0xEF4C` | `B+0x148` | Same upper-bound search |
| `0xF094` | `B+0x80` | Same upper-bound search |

These searches test one endpoint. The common frequency-rejection predicate instead tests closed-range membership in both `B+0x30` and `B+0x58`: signed raw kind below 16 yields `!(hit30 && hit58)`; kind at least 16 yields false.

### 4.3 User-Edge Generation

The user-edge generator fills 30 low/high DWORD pairs at `G[0x378C0,0x379B0)`. Captured seed selector 0 selects `B+0xF8`; other selectors select `B+0xD0`. Each iteration rereads the container count and obtains low and high through separate getter calls.

An unavailable endpoint writes `(0,0)` and retains `previous_high`. For a later input satisfying `u32(low-1) == previous_high`, the generator extends the preceding output's high without advancing the output position or updating `previous_high`. Otherwise it writes a new pair and remembers its high.

```text
Input:  (10,20), (21,30), (31,40)
Output: (10,30), (31,40), (0,0), ...
```

### 4.4 Document Normalization

The typed pass walks these inclusive index ranges in table order:

- `0x02`, indices 0–19999: Normal side rows.
- `0x03`, indices 0–7: Call side rows.
- `0x04`, indices 0–44: Band stacking.
- `0x05`, indices 1–10: Memopad.
- `0x08`, indices 0–24: Program scan.
- `0x15`, indices 0–29: User band edges.
- `0x16`, indices 0–67: Fixed band edges.
- `0x18`, indices 0–11: Auto repeater ranges.
- `0x19`, `0x1A`, `0x1B`, indices 0–1, 0–5, 0–1: Paired preset records.

Each valid record loads the same type/index from live `F`, applies corrections, writes, and reloads. Invalid records skip this typed work. Type `0x18` is skipped when captured `Q[0x13] != 1` or a seed-selector read at entry yields 0.

Normal, call, band-stacking, memopad, scan, and paired-preset records receive offset rounding. For these types, full-DWORD mode outside `0`, `1`, `8`, `0x0D` clears DATA. Mode low byte `0x0F`/`0x11` forces filter `0xFF` for scan and 0 for types `0x02`–`0x05` and `0x19`–`0x1B`. With `Q[0x12] == 0`, those non-scan types change TX/RX tone value 50 to 24.

For normal/call side 1 with split 0 and `VA 0x864614 == 0`, rejection by the specific channel predicate reloads index minus one and restores the original destination index before writing. That predicate rejects common-range failures, full-DWORD mode `0x18`, and, for call memory, frequencies outside `B+0x120[aux]`; `aux` is 12 for indices 0–3 and 13 for 4–7. The preceding side's offset is retained without another rounding pass.

Memopad processing compacts holes and restores decoded defaults when all displayed entries are holes. Later passes process repeaters, preset templates, references, and scalar windows. The final writes restore `F[0x3A8C8]` and the 16-byte marker at `[0x3C9B0,0x3C9C0)`.

## 5. Document Operations

### 5.1 Dirty State and Pending Save

While dirty is 0, comparison checks current against baseline by length and every byte, including the working tail. A mismatch sets dirty to 1. Further comparisons are skipped until a lifecycle operation resets it, so editing back to baseline leaves dirty set.

The pending-save dialog maps Yes to Save, No to continuation, and Cancel to termination of the requested operation. Save failure maps Yes to Cancel. Synchronization and pending-text finalization performed before this dialog remain in effect.

### 5.2 New and Open

New sets target 6, copies `G` to current, establishes baseline, clears dirty and path, rebuilds the display, and sends the completion event. It uses the existing generated default and leaves pending document slots to their individual handlers.

Ordinary open runs:

```text
Pending-save gate -> file selection -> private reader image
    -> reader status 0 -> signature 0 -> exact coverage 0
    -> snapshot / reset to M / copy
    -> profile comparison and confirmation
    -> convert to layout 6 -> adopt current
    -> load metadata / normalize / rebuild view
    -> document bookkeeping
```

Profile confirmation Cancel stops before replacing current. The capability-adoption branch regenerates `G` before conversion; the branch retaining the existing capability converts against the existing `G`. The profile-mismatch No branch continues loading with an empty document path and dirty 1. The selector retained for document target and the selector read immediately before conversion are separate reads.

Non-COM ICF initial-information loading begins with a private copy of current, reads and converts it using the old `G`, decodes final capability metadata, regenerates `G`, and copies the regenerated default into current.

### 5.3 Save

1. Obtain validation error and warning counts.
2. Normalize current in place.
3. Apply optional correction selected by the validation response.
4. Copy current to private `S` and rebuild `S` metadata.
5. Convert `S` from layout 6 to target and write the file.
6. On writer success with raw target other than 6, recopy current, rebuild metadata, convert `6 -> target -> 6`, adopt the result, and reload current.
7. Update path/title, target, baseline, and dirty.

Nonzero validation errors display a message and select correction/CD flags `(0,0)`. With no errors and some warnings, Yes selects `(1,1)` and other responses `(0,0)`. With neither errors nor warnings, the flags are `(0,1)`.

The post-write round trip starts from a fresh copy of current. Older layouts drop or replace unsupported state in both the file and the post-save document. Writer failure skips the round trip and final bookkeeping, retaining preceding normalization and any partial file. Exceptions during later title, view, or notification work leave the already-completed file and state updates in place. The writer discards `fclose`'s result; a close-only failure can finish with save status 0 and dirty 0.

### 5.4 Target Selection

The converter maps selectors outside `1..6` to local 6 while Save retains the raw target DWORD. Legacy confirmation and the post-write round trip test raw target against 6. Target 255 therefore enters both branches, writes MapRev 6, and remains 255 in document bookkeeping.

Legacy confirmation Cancel returns save failure; No returns the special result that reopens Save As. Save As translates its one-based filter index to a target, mapping out-of-range indices to the first entry, target 6.

## 6. MapRev Conversion

### 6.1 Conversion Steps

The converter takes destination and source selectors, normalizes each to `1..6` with local fallback 6, and allocates `M` bytes for every layout. Equal selectors copy the object. Different selectors traverse adjacent steps through private intermediate objects; each step zeroes its destination before copying and setting fields. Added fields can read live `G`.

| Transition | Structural and Value Changes |
| --- | --- |
| `1 -> 2` | Insert `0x400` bytes, expand 17 records from 24 to 32 bytes, remap enum values |
| `2 -> 1` | Shrink those structures, replace unsupported enum values from `G` |
| `2 -> 3` | Relocate windows, set fixed bytes, move the marker, clear `0x39724` |
| `3 -> 2` | Reverse selected moves, clear `0x384E4` |
| `3 -> 4` | Expand 50 records from 47 to 48 bytes, initialize added bytes from `G` or literal 1 |
| `4 -> 3` | Drop each added record byte |
| `4 -> 5` | Insert `G[0x399C1,0x399DD)`; relocate later windows and marker |
| `5 -> 4` | Drop those 28 bytes; conditionally replace an eight-byte window with spaces |
| `5 -> 6` | Copy image, clear `[0x3A318,0x3A31B)` and byte `0x39724`, set `0x355A1` to 6 |
| `6 -> 5` | Same clears; reduce `0x399BB >= 2` to 1 and `0x399C0 >= 3` to 2; set `0x355A1` to 5 |

For `3 -> 4`, added destination byte `0x30A24+48*i` takes the corresponding live-default byte, except that source DWORD `0x30A19+47*i == 0` selects literal 1. For `5 -> 4`, source byte `0x395A8 != 0x20` fills destination `[0x395A2,0x395AA)` with spaces.

### 6.2 Marker Placement and Object Copy

The `4 -> 5` step moves 48 bytes from `0x3C980` to `0x3D400` and writes the marker at `0x3D430`, beyond the file payload. The ordinary marker window at `0x3C9B0` is zero immediately after that step. Document normalization later writes the marker there.

Object copy resets the destination before reading source bytes. Source/destination aliasing therefore zeroes the source during self-copy. Open and Save construct separate source and destination objects.

## 7. ICF Format

### 7.1 File Structure

ICF is line-oriented text: model/revision header, metadata comments, addressed hex records, and an optional CD comment. The writer uses CRT text mode, uppercase hex, 32 payload bytes per data line, and CRLF output. A synthetic reader input excerpt with a four-byte data record is:

```text
40750001
#Comment=EXAMPLE
#MapRev=6
#EtcData=00000000
0000000004007A7F80
```

The header encodes model `0x4075` and revision 1. The data line is address `00000000`, byte count `04`, and bytes `00 7A 7F 80`. File coverage requires the entire layout-specific payload:

| MapRev | Inclusive Payload Range | Bytes | Internal Revision Offset | Marker Offset |
| --- | --- | --- | --- | --- |
| 1 | `0..0x3ACFF` | `0x3AD00` | `0x3516F` | `0x3ACF0` |
| 2 | `0..0x3B0FF` | `0x3B100` | `0x3556F` | `0x3B0F0` |
| 3 | `0..0x3C9BF` | `0x3C9C0` | `0x3556F` | `0x3C9B0` |
| 4, 5, 6 | `0..0x3C9BF` | `0x3C9C0` | `0x355A1` | `0x3C9B0` |

The marker is the 16-byte ASCII string `IcomCloneFormat3`. The internal revision check accepts a signed byte in `1..6` independently of external MapRev. A separate argument enables the marker comparison. Ordinary open requires reader status 0, signature result 0, and coverage result 0, in that order.

### 7.2 Reader Grammar and Metadata

The reader opens in CRT text mode and calls `fgets` with a `0x300`-byte buffer, yielding at most 767 bytes per fragment. CRT translation removes CR and stops at CTRL-Z. Embedded NUL is copied into the buffer and truncates later C-string processing. Long physical lines are handled as separate fragments; a BOM remains part of the input.

The first C-string has length nine. Its first eight bytes must pass decimal-digit classification and are combined as two four-digit hexadecimal fields; the ninth byte is unchecked. Model mismatch returns 3 before revision mismatch returns 4.

Lines beginning with `#` are comments. Case-sensitive search checks `MapRev=` first, then `EtcData=`; value parsing starts at the fixed offset from the line beginning. MapRev uses a signed decimal-prefix parser with 32-bit wrap. EtcData uses a signed hexadecimal scanner with saturation: `FFFFFFFF` yields `0x7FFFFFFF`, and `-80000000` yields `0x80000000`. Other comments, including `#CD`, are ignored.

MapRev and capability callbacks run once, immediately before the first non-comment record. Later comments update final metadata without rerunning callbacks. Reader construction sets MapRev to `0xFFFFFFFF` and EtcData to 0; subsequent calls retain metadata until overwritten.

Each data fragment loses its last byte before parsing. The remaining C-string must have length 12–520, contain only hex digits, and satisfy `length == 10 + 2*count`: address8, count2, payload. Every destination address is resolved before writing. Duplicate addresses overwrite earlier values. A final line without a newline loses its final hex digit.

Body read error and clean EOF take the same completion path. Complete coverage acquired before a read error can still pass open. A nonzero reader status restores its image snapshot while retaining metadata changes and callback effects.

### 7.3 CD Generation

When enabled, the writer emits `#CD=` followed by uppercase MD5 hex. The hash input is:

```text
44 00 77 55 40 75
    || BE32(address_0) || u8(count_0) || payload_0
    || BE32(address_1) || u8(count_1) || payload_1
    || ...
```

Addresses, lengths, and payloads enter the hash as binary bytes. Text headers, comments, hex spelling, and line endings are excluded. The reader ignores CD, checks EtcData through its capability callback, and checks the image revision/marker and coverage through the outer open path.

### 7.4 Coverage

Coverage uses one LSB-first bit per address: byte `a >> 3`, bit `a & 7`. Bitmap length is `ceil(actual_address_map_length/8)`. Required inclusive ranges are consumed until the first reversed range or invalid endpoint; padding bits also participate in comparison.

- **0:** Exact required bitmap.
- **7:** Missing bits only, or an explicit null coverage pointer.
- **8:** Extra bits, including mixed missing/extra.

## 8. Clone Protocol

### 8.1 Frames and Commands

Clone frames have the form `FE FE destination source command body FD`, with optional extra FE preamble bytes. PC-to-radio destination/source is `EE EF`; radio-to-PC is `EF EE`. Frame extraction collapses the FE preamble to one FE and removes FD before command processing.

| Command | Body and Processing |
| --- | --- |
| `E0` | Identification request: BE16 model, BE16 revision |
| `E1` | Identification response: BE16 model, BE16 revision, capability tail |
| `E2` | Read request: same body as E0 |
| `E3` | Write request: same body as E0 |
| `E4` | Escaped address, count, transformed image bytes, additive checksum |
| `E5` | Nine-byte ASCII `Icom Inc.`, optionally followed by two uppercase hex digits encoding E4 block count modulo 256 |
| `E6` | Acknowledgment: status byte; exact normalized success vector `FE EF EE E6 00` |

The usual request tuple is model `0x4075`, revision 0; the expected response revision is 1.

```text
Identification Request:  FE FE EE EF E0 40 75 00 00 FD
Read Request:            FE FE EE EF E2 40 75 00 00 FD
Write Request:           FE FE EE EF E3 40 75 00 00 FD
Acknowledgment on Wire:  FE FE EF EE E6 00 FD
After Frame Extraction:     FE EF EE E6 00
```

### 8.2 Data Encoding

The E4 body before escaping is `BE32(address) || u8(count) || transformed_data || checksum`. Each image byte becomes `T = (byte + 0x80) & 0xFF`; checksum is the negative sum of address bytes, count, and transformed data modulo 256. Body bytes `0xFA..0xFF` encode as `FF` followed by their low nibble. Header and terminator remain unescaped.

```text
Address:           0x1234
Image Bytes:       00 7A 7F 80
Transformed Bytes: 80 FA FF 00
Checksum:          3D
Wire Frame:        FE FE EE EF E4 00 00 12 34 04 80 FF 0A FF 0F 00 3D FD
```

Receive unescaping treats FF as an escape prefix and restores the next non-FF byte with OR `0xF0`. Repeated FF bytes keep the escape pending; a trailing FF leaves it pending at the end of the body.

### 8.3 Identification and Profiles

Upper-level E1 processing checks model, revision, tail length at least 33, `tail[0] == tail.length`, and selector `tail[1]` in `1..6`. It saves the model word before later checks and copies the first 33 tail bytes before checking the selector. An invalid selector sets an error and continues context comparison, which can replace that result.

The tail mapper updates 12 semantic fields in `Q`, retaining padding and options at `Q[0x14,0x18)`, including override. CloneInfo text uses saved `tail[2,18)`; its display/editor path leaves the saved tail unchanged.

Every profile allocates `M` bytes. Profile 1 transfers inclusive range `0..0x3ACFF`, profile 2 `0..0x3B0FF`, and profiles 3–6 `0..0x3C9BF`. Write options form the transfer target by subtracting profile-defined exclusion ranges.

### 8.4 Controller and Port Output

```text
Idle -> Prepare -> Port Setup/Open
                      -> Probe: E0 -> E1
                      -> Read:  E2 -> E4* -> E5
                      -> Write: E3 -> E4* -> E5 -> E6
                   -> Pending Drain -> Close -> Callback
```

Document read first runs an E0/E1 probe, then starts a separate E2/data/end operation. A controller start returns 1 after accepting the request. A busy controller returns 0 with its existing result object intact.

Protocol sends enqueue bytes and ignore the enqueue result. The port worker writes up to `0x400` bytes; an actual count below the requested count sets port error and retains the queue. Pending output is the u32 sum of software-queue length and hardware output count. The controller timer replaces earlier results with port-error result 17 when port error is nonzero.

Normal write uses an image chunk cap of 64 bytes. Within an inclusive target range:

```text
count = min(cap - address % cap, high - address + 1)
```

Address, block count, and progress counters advance during packet construction. Output draining precedes close and callback processing.

### 8.5 Receive State and Coverage

E4 processing checks body length, checksum, and every destination address before copying image bytes. It then sets coverage logical length to `ceil((address+count)/8)`, sets the packet's bits, and adds count to the cumulative byte counter, including duplicates.

Coverage can shrink on a later lower-address packet. Shrink and re-extension within capacity retain backing bytes; reallocation copies only the current logical length and fills the remainder. Zero-length packets still set coverage length. Failure during coverage processing retains the preceding image copy.

- **0:** Exact required coverage.
- **8:** Missing only, or null result or range input.
- **9:** Extra/mixed bits or excessive vector length.

E5 processing compares its body with `Icom Inc.` and the selected optional block count. Prefix/length mismatch returns 6, remaining mismatch returns 7, and equality returns 0. Successful E5 and coverage checks lead to document signature validation. On success, the document adopts saved capability, regenerates `G`, converts the borrowed receive image using saved `tail[1]`, and destroys the result object after the callback.

E1/E6 extraction returns the last completed frame from each read, carrying any partial frame forward. The DATA-state caller processes queued frames only when a new read returns positive bytes. Closing the progress UI clears UI callback state while the controller continues its operation.

## 9. CSV Format and Conversion

### 9.1 Lines, Cells, and Headers

CSV is a byte-oriented separator dialect. Physical lines split on CR, LF, or CRLF, then cells split on the configured separator. Quotes are ordinary bytes: `"a,b",c` with comma separator produces three cells. Fixed DBCS lead ranges `0x81..0x9F` and `0xE0..0xEF` cause the separator scanner to skip the following byte. Trailing-byte validity is unchecked; BOM and charset conversion are absent.

CSV decimal/list separators are copied application state. VCL formatting reads live locale state; CRT parsing and character classification use their own locale state; ANSI/NLS helpers use their individual Windows calls. Generic fixed-text export replaces both comma and semicolon with spaces and trims the right edge. Callsign export uses C-prefix/trim processing without separator replacement. Embedded CR/LF is emitted directly.

Header binding caches the first normalized label match. A missing column has index -1; a present column beyond a short row yields an empty string. GPS uses Group whenever that column exists, including an empty cell, and uses Bank only when Group is missing. Repeater's required header is `Repeater Call Sign`.

### 9.2 Values and Text

| Value | Conversion |
| --- | --- |
| Frequency/offset | MHz, six fractional digits; numeric prefilter rejects plus signs and exponent notation |
| DUP | Values 3 and 4 both export as `RPS`; import chooses first match, value 3 |
| Fixed-width text | Byte-counted, space-padded; overlength truncation observes fixed DBCS boundaries |
| Exact-width text | Retained even with a dangling DBCS lead byte |
| Coordinates | Degree byte, BE16 minute fraction in 1/60000-degree units, ASCII hemisphere; CSV uses signed decimal degrees |
| Altitude | Signed 0.1-foot units, missing `0x7FFFFFFF`; CSV uses fixed metric conversion |
| Date/time | BCD fields; CSV formatting also reads time offset and locale state |

Numeric conversion passes through the host floating-point parser, scale, integer conversion, low-word extraction, and caller-specific rounding. Fixed text's raw storage can contain NUL; C-prefix display and export stop there.

### 9.3 Row Schemas

Repeater export produces 17 logical cells and 16 separators:

```text
GroupNo, GroupName, Name, SubName, RepeaterCall, GatewayCall,
Frequency, DUP, Offset, Mode, Tone, RepeaterTone,
USE, Position, Latitude, Longitude, UTCOffset
```

Frequency 0 produces an empty cell. Modes other than 0/1 produce empty Tone and RepeaterTone cells. GroupNo comes from the separate group argument `g+1`. Invalid holders produce 17 empty cells; empty groups produce no row.

Normal-memory export buffers side A of a split row, then appends side B and writes one line. With boolean `groups` enabling the two group-prefix cells and `sel` enabling the SEL cell, unsplit output has `18+2*groups+sel` cells and combined split output has `35+2*groups+sel`, up to 38. Side A alone has no line terminator.

Callsign-list export reads Name directly at `I+0x2F703+16*row` and Callsign at `I+0x2EDA3+8*row`, obtaining the live image pointer separately for each read.

### 9.4 Import, Export, and Cancellation

Import maintains status, discard, correction, and valid/event counters. One row can increment both discard and correction; capacity stop can overwrite the valid/event counter with 1. Pair imports can write a provisional lower record before accepting the final pair.

File-dialog cancellation exits before document normalization. After file selection, confirmation cancellation and reader failure still reach live-current normalization. Whole/replace import deletes destination state before reading and retains that deletion when input is empty or invalid.

Export normalizes current before output, opens with truncation, and writes logical byte lengths. Some writer paths continue after OS BOOL success with a short byte count; close results are ignored. Failed output leaves the partial file in place.
