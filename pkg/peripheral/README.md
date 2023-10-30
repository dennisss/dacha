

Air Quality

- SHT31-D
    - Temp + Humidity

- BME680
    - TEmp, humidity, baro, and VOc

- SGP30 (MOX)
    - QT
    - https://www.adafruit.com/product/3709
    - eCO2 (equivalent calculated carbon-dioxide) concentration within a range of 400 to 60,000 parts per million (ppm), and TVOC (Total Volatile Organic Compound) concentration within a range of 0 to 60,000 parts per billion (ppb).
    - Need humidity sensor for calibration


- MiCS5524
    - https://www.adafruit.com/product/3199
    - This sensor is sensitive to CO ( ~ 1 to 1000 ppm), Ammonia (~ 1 to 500 ppm), Ethanol (~ 10 to 500 ppm), H2 (~ 1 - 1000 ppm), and Methane / Propane / Iso-Butane (~ 1,000++ ppm). However, it can't tell you which gas it has detected. 

- CCS811
    - VOX/eCO2
    - https://www.adafruit.com/product/3566
    - This part will measure eCO2 (equivalent calculated carbon-dioxide) concentration within a range of 400 to 8192 parts per million (ppm), and TVOC (Total Volatile Organic Compound) concentration within a range of 0 to 1187 parts per billion (ppb). According to the fact sheet it can detect Alcohols, Aldehydes, Ketones, Organic Acids, Amines, Aliphatic and Aromatic Hydrocarbons.

- PMSA003I
    - 


- BMP388
    - QT
    - https://www.adafruit.com/product/3966
    - Temperature + barometric pressure.

- Past work:
    - https://hackaday.io/project/167424-smart-3d-printer-emission-monitor
        - CCS811 (MOX) 
       

Air quality sensors to have:
- 1 inside 3d printer enclosure (suspending in air)
- 1 above 3d printer enclosure
- 1 near desk
- 1 in bedroom


Printer Air Filter Module:
- 130mm x 160mm cutout

- Uses 1 PWM (requires disabling audio if using RPi) for the fan
- Also want to read in the fan speed
- In parallel
    - Monitor air quality inside the enclosure
    - Control the 3d printers via USB
    - LEDs
    - Cameras
    - Power monitoring + power on/off
