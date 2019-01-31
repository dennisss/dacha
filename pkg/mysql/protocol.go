package main

// NOTE: Throughout this, we assume Protocol 41

import (
	"fmt"
	"bufio"
	"bytes"
	"encoding/binary"
	"reflect"
)

// https://dev.mysql.com/doc/internals/en/integer.html
type Int1 uint8
type Int2 uint16
type Int3 uint32
type Int4 uint32
type Int6 uint64
type Int8 uint64
type IntVar uint64

// https://dev.mysql.com/doc/internals/en/string.html
type StringFix string
type StringNUL string
type StringVar string
type StringLenEnc string
type StringEOF string




func readInt1(buf *bytes.Buffer) (v uint8) {
	b, _ := buf.ReadByte()
	v = uint8(b)
	return
}
func writeInt1(buf *bytes.Buffer, v uint8) {
	binary.Write(buf, binary.LittleEndian, v)
	return
}

func readInt2(buf *bytes.Buffer) (v uint16) {
	binary.Read(buf, binary.LittleEndian, &v)
	return
}
func writeInt2(buf *bytes.Buffer, v uint16) {
	binary.Write(buf, binary.LittleEndian, v)
	return
}

func readInt3(r *bytes.Buffer) (v uint32) {
	b := make([]byte, 3)
	r.Read(b)
	b = append(b, 0)
	buf := bytes.NewReader(b)
	binary.Read(buf, binary.LittleEndian, &v)
	return
}
func writeInt3(w *bytes.Buffer, v uint32) {
	b :=  new(bytes.Buffer)
	wb := bufio.NewWriter(b)
	binary.Write(wb, binary.LittleEndian, v)
	wb.Flush()
	w.Write(b.Bytes()[0:3])
	return
}

func readInt4(buf *bytes.Buffer) (v uint32) {
	binary.Read(buf, binary.LittleEndian, &v)
	return
}
func writeInt4(buf *bytes.Buffer, v uint32) {
	binary.Write(buf, binary.LittleEndian, v)
	return
}

func readInt6(r *bytes.Buffer) (v uint64) {
	b := make([]byte, 6)
	r.Read(b)
	b = append(b, 0, 0)
	buf := bytes.NewReader(b)
	binary.Read(buf, binary.LittleEndian, &v)
	return
}
func writeInt6(w *bytes.Buffer, v uint64) {
	b :=  new(bytes.Buffer)
	wb := bufio.NewWriter(b)
	binary.Write(wb, binary.LittleEndian, v)
	wb.Flush()
	w.Write(b.Bytes()[0:6])
	return
}

func readInt8(buf *bytes.Buffer) (v uint64) {
	binary.Read(buf, binary.LittleEndian, &v)
	return
}
func writeInt8(buf *bytes.Buffer, v uint64) {
	binary.Write(buf, binary.LittleEndian, v)
	return
}

func readIntVar(buf *bytes.Buffer) uint64 {
	var i uint64 = 0
	b, _ := buf.ReadByte()
	if b < 0xfb {
		i = uint64(b)
	} else if b == 0xfc {
		i = uint64(readInt2(buf))
	} else if b == 0xfd {
		i = uint64(readInt3(buf))
	} else if b == 0xfe {
		i = readInt8(buf)
	} else {
		panic("Unknown var int")
	}
	
	return i;
}
func writeIntVar(buf *bytes.Buffer, v uint64) {
	if v < 0xfb {
		buf.WriteByte(byte(v))
	} else if v <= 0xffff {
		buf.WriteByte(0xfc)
		writeInt2(buf, uint16(v))
	} else if v <= 0xffffff {
		buf.WriteByte(0xfd)
		writeInt3(buf, uint32(v))
	} else {
		buf.WriteByte(0xfe)
		writeInt8(buf, uint64(v))
	}
}

func readStringNul(buf *bytes.Buffer) string {
	str, _ := buf.ReadString(byte(0))
	str = str[0:len(str) - 1]
	return string(str)
}

func readStringVar(buf *bytes.Buffer, size uint) string {
	b := make([]byte, size)
	buf.Read(b)
	return string(b)
}



// TODO: Also return an error
func Marshal(v interface{}) []byte {
	typ := reflect.TypeOf(v).Elem()
	val := reflect.ValueOf(v).Elem()
	
	var buf bytes.Buffer
	
	for i := 0; i < val.NumField(); i++ {
		f := val.Field(i)
				
		switch f.Interface().(type) {
			case Int1:
				writeInt1(&buf, uint8(f.Uint()))
			case Int2:
				writeInt2(&buf, uint16(f.Uint()))
			case Int3:
				writeInt3(&buf, uint32(f.Uint()))
			case Int4:
				writeInt4(&buf, uint32(f.Uint()))
			case Int6:
				writeInt6(&buf, uint64(f.Uint()))
			case Int8:
				writeInt8(&buf, uint64(f.Uint()))
			case IntVar:
				writeIntVar(&buf, uint64(f.Uint()))
			case StringFix:

				var size int
				fmt.Sscanf(typ.Field(i).Tag.Get("mysql"), "len=%d", &size)

				if len(f.String()) != size {
					fmt.Println("Fixed length string wrong length", len(f.String()), "!=", size)
				}
				
				buf.WriteString(f.String())
				
			case StringVar:
				buf.WriteString(f.String())
				
			case StringNUL:
				buf.WriteString(f.String() + "\x00")
			case StringLenEnc:
				writeIntVar(&buf, uint64(len(f.String())))
				buf.WriteString(f.String())
			case StringEOF:
				if i != val.NumField() - 1 {
					panic("StringEOF not at end of struct")
				}
				buf.WriteString(f.String())
			default:
				fmt.Println(i, f.Kind(), f.Type())
				panic("Unknown protocol basic type while marshaling")
		}
	}
	
	return buf.Bytes()
}

// TODO: Would it be more efficient to pass in a pointer to the bytes?
func Unmarshal(data []byte, v interface{}) {
	typ := reflect.TypeOf(v).Elem() 
	val := reflect.ValueOf(v).Elem()
	
	buf := bytes.NewBuffer(data)	
	
	for i := 0; i < val.NumField(); i++ {
		f := val.Field(i)
		
		switch f.Interface().(type) {
			case Int1:
				i := readInt1(buf)
				f.SetUint(uint64(i))
			case Int2:
				i := readInt2(buf)
				f.SetUint(uint64(i))
			case Int3:	
				i := readInt3(buf)
				f.SetUint(uint64(i))
			case Int4:
				i := readInt4(buf)
				f.SetUint(uint64(i))
			case Int6:
				i := readInt6(buf)
				f.SetUint(uint64(i))
			case Int8:
				i := readInt8(buf)
				f.SetUint(uint64(i))
			case IntVar:
				i := readIntVar(buf)
				f.SetUint(uint64(i))
				
			case StringFix:
				var size uint
				fmt.Sscanf(typ.Field(i).Tag.Get("mysql"), "len=%d", &size)
				b := make([]byte, size)
				buf.Read(b)
				f.SetString(string(b))
			case StringNUL:
				str := readStringNul(buf)
				f.SetString(str)
			// case StringVar
				
			case StringLenEnc:
				size := readIntVar(buf)
				b := make([]byte, size)
				buf.Read(b)
				f.SetString(string(b))
			case StringEOF:
				b := make([]byte, buf.Len())
				buf.Read(b)
				f.SetString(string(b))
			
			default:
				fmt.Println(i, f.Kind(), f.Type())
				panic("Unknown protocol basic type while unmarshaling")


		}
	}
	
} 

func UnmarshalPacket(data []byte, v *Packet) {
	buf := bytes.NewBuffer(data)
	
	v.PayloadLength = Int3(readInt3(buf))
	v.SequenceId = Int1(readInt1(buf))
	v.Payload = StringVar(readStringVar(buf, uint(v.PayloadLength)))
}



type Packet struct {
	PayloadLength Int3 // int<3>
	SequenceId Int1 // int<1>
	Payload StringVar // string<var>
}



// HandshakeV10: https://dev.mysql.com/doc/internals/en/connection-phase-packets.html#packet-Protocol::Handshake
// TODO: There are also some other optional fields in this
type Handshake struct {
	ProtocolVersion Int1
	ServerVersion StringNUL
	ConnectionId Int4
	AuthPluginDataPart1 StringFix `mysql:"len=8"`
	Filler1 Int1
	CapabilityFlag1 Int2
	// Optional below here	
	CharacterSet Int1
	StatusFlags Int2
	CapabilityFlags2 Int2
	AuthPluginDataLen Int1
	Reserved StringFix `mysql:"len=10"` // all zero 
	
	AuthPluginDataPart2 StringVar `mysql:CLIENT_SECURE_CONNECTION` // if capabilities & CLIENT_SECURE_CONNECTION
	//^ string[$len]   auth-plugin-data-part-2 ($len=MAX(13, length of auth-plugin-data - 8))
	AuthPluginName StringNUL `mysql:CLIENT_PLUGIN_AUTH` // if capabilities & CLIENT_PLUGIN_AUTH
}

// HandshakeResponse41: https://dev.mysql.com/doc/internals/en/connection-phase-packets.html#packet-Protocol::HandshakeResponse
type HandshakeResponse struct {
	CapabilityFlag Int4
	MaxPacketSize Int4
	CharacterSet Int1
	Reserved StringFix `mysql:"len=23"` // All zero
	Username StringNUL
	AuthResponse StringLenEnc // if capabilities & CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA  (TODO: There are two other alternatives to this)
	
	Database StringNUL // if capabilities & CLIENT_CONNECT_WITH_DB
	AuthPluginName StringNUL // if capabilities & CLIENT_PLUGIN_AUTH
	// ... More stuff we don't really care about
}


// https://dev.mysql.com/doc/internals/en/packet-OK_Packet.html
// NOTE: This assumes that we don't implement CLIENT_SESSION_TRACK
type OKPacket struct {
	Header Int1
	AffectedRows IntVar
	LastInsertId IntVar
	StatusFlags Int2
	Warnings Int2
	Info StringEOF
}

type EOFPacket struct {
	Header Int1
	Warnings Int2
	StatusFlags Int2
}

type ERRPacket struct { // ERR_Packet
	Header Int1
	ErrorCode Int2
	SqlStateMarker StringFix `mysql:"len=1"`
	SqlState StringFix `mysql:"len=5"`
	ErrorMessage StringEOF
}

/*
func UnmarshalHandshakeResponse(data []byte, v *HandshakeResponse) {
	buf := bytes.NewBuffer(data)
	
	v.CapabilityFlag = readInt4(buf)
	v.MaxPacketSize = readInt4(buf)
	v.Reserved = readStringVar(buf, 23)
	v.Username = readStringNul(buf)
	
	if v.CapabilityFlag & CLIENT_PLUGIN_AUTH_LENENC_CLIENT_DATA {
		v.AuthResponse = 
	}
	
	if v.CapabilityFlag & CLIENT_CONNECT_WITH_DB {
		v.Database = 
	}

}
*/
