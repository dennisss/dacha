

const byte kDefaultSpeed = 255;  // 80%
const unsigned long kTimeoutMs = 5000;

void setup() {
  pinMode(10, OUTPUT);
}

void loop() {
  TCCR1B = TCCR1B & B11111000 | B00000001;
  
  byte current_speed = kDefaultSpeed;
  analogWrite(10, current_speed);
  
  unsigned long last_update_time = millis();
  unsigned long now;

  while (true) {
    byte next_speed = 0;
    bool next_speed_received = false;
    while (Serial.available() > 0) {
      next_speed = Serial.read();
      next_speed_received = true;
    }

    now = millis();
    
    if (next_speed_received) {
      current_speed = next_speed;
      last_update_time = now;
      Serial.print(next_speed);
      Serial.write('\n');
    } else if (now - last_update_time >= kTimeoutMs) {
      current_speed = kDefaultSpeed;
    }

    analogWrite(10, current_speed);
  
    delay(100); // 0.1 seconds
  }
}
