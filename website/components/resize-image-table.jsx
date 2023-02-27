export const ResizeImageTable = () => (
  <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'flex-start' }}>
    <div style={{ margin: '0 8px 8px 0' }}>
      <img
        src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-near.png"
        title="Nearest"
        alt="Nearest"
      />
      <br />
      Nearest Neighbor
    </div>
    <div style={{ margin: '0 8px 8px 0' }}>
      <img
        src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-tri.png"
        title="Triangle"
        alt="Triangle"
      />
      <br />
      Linear: Triangle
    </div>
    <div style={{ margin: '0 8px 8px 0' }}>
      <img
        src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-cmr.png"
        title="CatmullRom"
        alt="CatmullRom"
      />
      <br />
      Cubic: Catmull-Rom
    </div>
    <div style={{ margin: '0 8px 8px 0' }}>
      <img
        src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-gauss.png"
        title="Gaussian"
        alt="Gaussian"
      />
      <br />
      Gaussian
    </div>
    <div style={{ margin: '0 8px 8px 0' }}>
      <img
        src="https://raw.githubusercontent.com/image-rs/image/master/examples/scaledown/scaledown-test-lcz2.png"
        title="Lanczos3"
        alt="Lanczos3"
      />
      <br />
      Lanczos with window 3
    </div>
  </div>
)
