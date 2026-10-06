export interface JpegHeader {
  /** Complete APP2 segments, ready to be copied into another JPEG. */
  iccSegments: Uint8Array[];
  /** ICC header colour space signature, e.g. "RGB ", "CMYK" or "GRAY". */
  iccColorSpace: string | undefined;
  /** 1 for greyscale, 3 for YCbCr/RGB, 4 for CMYK/YCCK. */
  components: number;
  /** EXIF orientation, 1 when absent. */
  orientation: number;
}

/** Reads the markers before the first scan; `undefined` for a malformed file. */
export function readJpegHeader(input: Uint8Array): JpegHeader | undefined {
  const iccSegments: Uint8Array[] = [];
  let components = 0;
  let orientation = 1;
  for (let offset = 2; offset + 4 <= input.byteLength && input[offset] === 0xff;) {
    const marker = input[offset + 1];
    if (marker === undefined || marker === 0xda || marker === 0xd9) {
      return { iccSegments, iccColorSpace: iccColorSpace(iccSegments), components, orientation };
    }
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) { offset += 2; continue; }
    const length = ((input[offset + 2] ?? 0) << 8) | (input[offset + 3] ?? 0);
    if (length < 2 || offset + 2 + length > input.byteLength) return undefined;
    const payload = input.subarray(offset + 4, offset + 2 + length);
    if (marker === 0xe2 && startsWith(payload, "ICC_PROFILE\0")) {
      iccSegments.push(input.slice(offset, offset + 2 + length));
    }
    if (marker === 0xe1 && startsWith(payload, "Exif\0\0")) {
      orientation = exifOrientation(payload.subarray(6)) ?? orientation;
    }
    // SOF0–SOF15 except DHT (C4), JPG (C8) and DAC (CC).
    if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
      components = payload[5] ?? 0;
    }
    offset += 2 + length;
  }
  return undefined;
}

/** Inserts complete marker segments right after SOI. */
export function insertSegments(jpeg: Uint8Array, segments: readonly Uint8Array[]): Uint8Array {
  if (!segments.length) return jpeg;
  const size = segments.reduce((total, segment) => total + segment.byteLength, 0);
  const output = new Uint8Array(jpeg.byteLength + size);
  output.set(jpeg.subarray(0, 2));
  let offset = 2;
  for (const segment of segments) { output.set(segment, offset); offset += segment.byteLength; }
  output.set(jpeg.subarray(2), offset);
  return output;
}

/** An APP1 segment carrying only the EXIF orientation tag. */
export function orientationSegment(orientation: number): Uint8Array {
  return new Uint8Array([
    0xff, 0xe1, 0x00, 0x22,
    ...[..."Exif\0\0"].map((char) => char.charCodeAt(0)),
    0x4d, 0x4d, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x08, // big-endian TIFF, first IFD at 8
    0x00, 0x01, // one entry
    0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, orientation & 0xff, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, // no next IFD
  ]);
}

export type RgbProfileKind = "srgb" | "display-p3" | "other";

/** D50-adapted colourant XYZ of the red, green and blue primaries. */
const PRIMARIES = {
  srgb: [[0.4361, 0.2225, 0.0139], [0.3851, 0.7169, 0.0971], [0.1431, 0.0606, 0.7141]],
  "display-p3": [[0.5151, 0.2412, -0.0011], [0.292, 0.6922, 0.0419], [0.1571, 0.0666, 0.7841]],
} as const;
const PRIMARY_TOLERANCE = 0.003;
/** sRGB transfer at 0.5; Display P3 uses the same curve. */
const SRGB_HALF = 0.214;
const TRC_TOLERANCE = 0.01;

/**
 * Classifies an RGB ICC profile by its primaries and red tone curve. sRGB
 * and Display P3 cover almost every web and phone photo; anything wider is
 * reported as "other".
 */
export function rgbProfileKind(segments: readonly Uint8Array[]): RgbProfileKind | undefined {
  const profile = joinProfile(segments);
  if (!profile || profile.byteLength < 132) return undefined;
  const view = new DataView(profile.buffer, profile.byteOffset, profile.byteLength);
  const tags = new Map<string, Uint8Array>();
  const count = view.getUint32(128);
  for (let index = 0; index < count && 144 + index * 12 <= profile.byteLength; index++) {
    const entry = 132 + index * 12;
    const offset = view.getUint32(entry + 4), size = view.getUint32(entry + 8);
    if (offset + size <= profile.byteLength) tags.set(ascii(profile.subarray(entry, entry + 4)), profile.subarray(offset, offset + size));
  }
  const primaries = ["rXYZ", "gXYZ", "bXYZ"].map((name) => xyz(tags.get(name)));
  if (primaries.some((value) => !value)) return "other";
  const half = transfer(tags.get("rTRC"), 0.5);
  const srgbCurve = half !== undefined && Math.abs(half - SRGB_HALF) <= TRC_TOLERANCE;
  for (const kind of ["srgb", "display-p3"] as const) {
    const matches = PRIMARIES[kind].every((expected, channel) =>
      expected.every((value, axis) => Math.abs(value - primaries[channel]![axis]!) <= PRIMARY_TOLERANCE));
    if (matches && srgbCurve) return kind;
  }
  return "other";
}

function joinProfile(segments: readonly Uint8Array[]): Uint8Array | undefined {
  if (!segments.length) return undefined;
  const ordered = [...segments].sort((a, b) => (a[16] ?? 0) - (b[16] ?? 0));
  const size = ordered.reduce((total, segment) => total + segment.byteLength - 18, 0);
  const profile = new Uint8Array(size);
  let offset = 0;
  for (const segment of ordered) { profile.set(segment.subarray(18), offset); offset += segment.byteLength - 18; }
  return profile;
}

function xyz(tag: Uint8Array | undefined): number[] | undefined {
  if (!tag || tag.byteLength < 20 || ascii(tag.subarray(0, 4)) !== "XYZ ") return undefined;
  const view = new DataView(tag.buffer, tag.byteOffset, tag.byteLength);
  return [0, 1, 2].map((index) => view.getInt32(8 + index * 4) / 65536);
}

/** Evaluates an ICC `curv` or `para` tone curve. */
function transfer(tag: Uint8Array | undefined, x: number): number | undefined {
  if (!tag || tag.byteLength < 12) return undefined;
  const view = new DataView(tag.buffer, tag.byteOffset, tag.byteLength);
  const type = ascii(tag.subarray(0, 4));
  if (type === "curv") {
    const count = view.getUint32(8);
    if (count === 0) return x;
    if (count === 1) return x ** (view.getUint16(12) / 256);
    if (tag.byteLength < 12 + count * 2) return undefined;
    const position = x * (count - 1), low = Math.floor(position), high = Math.min(count - 1, low + 1);
    const value = (index: number) => view.getUint16(12 + index * 2) / 65535;
    return value(low) + (value(high) - value(low)) * (position - low);
  }
  if (type === "para") {
    const kind = view.getUint16(8);
    const parameter = (index: number) => view.getInt32(12 + index * 4) / 65536;
    const g = parameter(0);
    if (kind === 0) return x ** g;
    const [a, b] = [parameter(1), parameter(2)];
    if (kind === 1) return x >= -b / a ? (a * x + b) ** g : 0;
    if (kind === 2) return x >= -b / a ? (a * x + b) ** g + parameter(3) : parameter(3);
    if (kind === 3) return x >= parameter(4) ? (a * x + b) ** g : parameter(3) * x;
    if (kind === 4) return x >= parameter(4) ? (a * x + b) ** g + parameter(5) : parameter(3) * x + parameter(6);
  }
  return undefined;
}

function ascii(bytes: Uint8Array): string {
  return String.fromCharCode(...bytes);
}

function iccColorSpace(segments: readonly Uint8Array[]): string | undefined {
  // The profile starts after the marker, length, "ICC_PROFILE\0" and the two
  // sequence bytes; its colour space signature is at profile offset 16.
  const first = segments.find((segment) => segment[16] === 1) ?? segments[0];
  if (!first || first.byteLength < 18 + 20) return undefined;
  return String.fromCharCode(...first.subarray(18 + 16, 18 + 20));
}

function exifOrientation(tiff: Uint8Array): number | undefined {
  if (tiff.byteLength < 8) return undefined;
  const view = new DataView(tiff.buffer, tiff.byteOffset, tiff.byteLength);
  const little = tiff[0] === 0x49;
  if (!little && tiff[0] !== 0x4d) return undefined;
  const ifd = view.getUint32(4, little);
  if (ifd + 2 > tiff.byteLength) return undefined;
  const entries = view.getUint16(ifd, little);
  for (let index = 0; index < entries; index++) {
    const entry = ifd + 2 + index * 12;
    if (entry + 12 > tiff.byteLength) return undefined;
    if (view.getUint16(entry, little) === 0x0112) {
      const value = view.getUint16(entry + 8, little);
      return value >= 1 && value <= 8 ? value : undefined;
    }
  }
  return undefined;
}

function startsWith(bytes: Uint8Array, text: string): boolean {
  if (bytes.byteLength < text.length) return false;
  for (let index = 0; index < text.length; index++) {
    if (bytes[index] !== text.charCodeAt(index)) return false;
  }
  return true;
}
