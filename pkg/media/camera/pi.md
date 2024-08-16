

Goal is to get a 1920x1080p video:


- Camera v1:
    - 2592x1944
    - 



## Raspberry Pi Camera V1

```
[0:00:30.701094832] [677]  INFO RPI vc4.cpp:446 Registered camera /base/soc/i2c0mux/i2c@1/ov5647@36 to Unicam device /dev/media3 and ISP device /dev/media1
Num Cameras: 1
Id: /base/soc/i2c0mux/i2c@1/ov5647@36
Static Num Streams: 3
Controls: ControlInfoMap {
    AwbEnable: [false..true] (default: <ValueType Error>),
    Saturation: [0.000000..32.000000] (default: 1.000000),
    StatsOutputEnable: [false..true] (default: false),
    FrameDurationLimits: [33333..120000] (default: <ValueType Error>),
    AeEnable: [false..true] (default: <ValueType Error>),
    ExposureTime: [0..66666] (default: <ValueType Error>),
    NoiseReductionMode: [0..4] (default: 0),
    AeMeteringMode: [0..3] (default: 0),
    AnalogueGain: [1.000000..16.000000] (default: <ValueType Error>),
    ScalerCrop: [(0, 0)/0x0..(65535, 65535)/65535x65535] (default: (0, 0)/0x0),
    AeConstraintMode: [0..3] (default: 0),
    ExposureValue: [-8.000000..8.000000] (default: 0.000000),
    Sharpness: [0.000000..16.000000] (default: 1.000000),
    AeExposureMode: [0..3] (default: 0),
    AeFlickerMode: [0..1] (default: 0),
    AwbMode: [0..7] (default: 0),
    AeFlickerPeriod: [100..1000000] (default: <ValueType Error>),
    ColourGains: [0.000000..32.000000] (default: <ValueType Error>),
    Brightness: [-1.000000..1.000000] (default: 0.000000),
    Contrast: [0.000000..32.000000] (default: 1.000000),
    HdrMode: [0..4] (default: 0),
}
Properties: ControlList {
    SystemDevices: "[ 20747, 20737, 20742, 20744 ]",
    ScalerCropMaximum: "(0, 0)/0x0",
    PixelArrayActiveAreas: "[ (16, 6)/2592x1944 ]",
    PixelArraySize: "2592x1944",
    Rotation: "0",
    Location: "2",
    ColorFilterArrangement: "2",
    UnitCellSize: "1400x1400",
    Model: "ov5647",
}
Acquired!
Supported Formats:
- NV21
- YUV420
- NV12
- YVU420
- XBGR8888
- BGR888
- RGB888
- XRGB8888
- RGB565
- YVYU
- YUYV
- VYUY
- UYVY
Size: Size { width: 800, height: 600 }
Pixel Format: NV21
[0:00:30.703599776] [672]  INFO Camera camera.cpp:1183 configuring streams: (0) 800x600-NV21
[0:00:30.704117424] [677]  INFO RPI vc4.cpp:621 Sensor: /base/soc/i2c0mux/i2c@1/ov5647@36 - Selected sensor format: 1296x972-SGBRG10_1X10 - Selected unicam format: 1296x972-pGAA
Configured!
Stride: 800
Stream: 800x600-NV21
Stream ID: 7fa0023d58
Request Controls: ControlList { AeEnable: "true" }
Request: Request(0:C:0/1:0)
Request_Status(1)
Planes: [FrameBufferPlane { fd: 18, offset: 0, length: 480000 }, FrameBufferPlane { fd: 18, offset: 480000, length: 240000 }]
Response Metadata: ControlList { FrameDuration: "66746", Lux: "70.528419", ExposureTime: "66654", AnalogueGain: "8.000000", SensorBlackLevels: "[ 1024, 1024, 1024, 1024 ]", AeLocked: "false", DigitalGain: "1.000000", ColourGains: "[ 1.288224, 1.825079 ]", ColourTemperature: "3489", FocusFoM: "911", ColourCorrectionMatrix: "[ 2.007312, -0.454313, -0.552999, -0.556340, 1.985763, -0.429423, -0.099411, -0.898531, 1.997932 ]", ScalerCrop: "(0, 0)/2592x1944", AeEnable: "true", SensorTimestamp: "31183253000" }

```