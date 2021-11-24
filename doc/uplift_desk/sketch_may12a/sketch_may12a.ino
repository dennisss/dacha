void setup() {
  Serial.begin(9600);
  Serial1.begin(9600, SERIAL_8N1);
}

void loop() {
  if(Serial.available() > 0) {
    int b = Serial.read();
    Serial1.write(241);
    Serial1.write(241);
    Serial1.write((uint8_t) 8);
    Serial1.write((uint8_t) 0);
    Serial1.write((uint8_t) 8);
    Serial1.write(126);
    Serial1.flush();
    Serial.print("Sent!");
  }
  if(Serial1.available() > 0) {
    
    int b = Serial1.read();
    Serial.print(b, DEC);
    Serial.print('\n');
  }
}
