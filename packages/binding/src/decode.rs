use napi::bindgen_prelude::*;

#[inline]
pub(crate) fn decode_input_image(input: &[u8]) -> Result<(Vec<u8>, u32, u32, bool)> {
  let file_type = infer::get(input).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "Unknown input buffer mime type".to_owned(),
    )
  })?;
  let file_mime_type = file_type.mime_type();
  match file_mime_type {
    "image/jpeg" => {
      let mut decoder = jpeg_decoder::Decoder::new(input);
      let decoded_buf = decoder.decode().map_err(|err| {
        Error::new(
          Status::GenericFailure,
          format!("Decode jpeg failed {}", err),
        )
      })?;
      let metadata = decoder.info().ok_or_else(|| {
        Error::new(
          Status::GenericFailure,
          "Get jpeg metadata failed".to_owned(),
        )
      })?;
      Ok((
        decoded_buf,
        metadata.width as u32,
        metadata.height as u32,
        false,
      ))
    }
    "image/png" => {
      let decoder = png::Decoder::new(input);
      let mut reader = decoder
        .read_info()
        .map_err(|err| Error::new(Status::InvalidArg, format!("Read png info failed {}", err)))?;
      let mut decoded_buf = vec![0; reader.output_buffer_size()];
      let output_info = reader
        .next_frame(&mut decoded_buf)
        .map_err(|err| Error::new(Status::InvalidArg, format!("Read png frame failed {}", err)))?;
      Ok((decoded_buf, output_info.width, output_info.height, true))
    }
    _ => Err(Error::new(
      Status::InvalidArg,
      format!("Unsupported input file type: {}", file_mime_type),
    )),
  }
}
