package main

import (
	"fmt"
)

const (
	COM_SLEEP = 0x00
	COM_QUIT = 0x01
	COM_INIT_DB = 0x02
	COM_QUERY = 0x03
	COM_FIELD_LIST = 0x04
	COM_CREATE_DB = 0x05
	COM_DROP_DB = 0x06
	COM_REFRESH = 0x07
	COM_SHUTDOWN = 0x08
	COM_STATISTICS = 0x09
	COM_PROCESS_INFO = 0x0a
	COM_CONNECT = 0x0b
	COM_PROCESS_KILL = 0x0c
	COM_DEBUG = 0x0d
	COM_PING = 0x0e
	COM_TIME = 0x0f
	COM_DELAYED_INSERT = 0x10
	COM_CHANGE_USER = 0x11
	COM_BINLOG_DUMP = 0x12
	COM_TABLE_DUMP = 0x13
	COM_CONNECT_OUT = 0x14
	COM_REGISTER_SLAVE = 0x15
	COM_STMT_PREPARE = 0x16
	COM_STMT_EXECUTE = 0x17
	COM_STMT_SEND_LONG_DATA = 0x18
	COM_STMT_CLOSE = 0x19
	COM_STMT_RESET = 0x1a
	COM_SET_OPTION = 0x1b
	COM_STMT_FETCH = 0x1c
	COM_DAEMON = 0x1d
	COM_BINLOG_DUMP_GTID = 0x1e
	COM_RESET_CONNECTION = 0x1f
)

// Protocol::MYSQL_TYPE_*: https://dev.mysql.com/doc/internals/en/com-query-response.html#column-type
const (
	MYSQL_TYPE_DECIMAL = 0x00
	MYSQL_TYPE_TINY = 0x01
	MYSQL_TYPE_SHORT = 0x02
	MYSQL_TYPE_LONG = 0x03
	MYSQL_TYPE_FLOAT = 0x04
	MYSQL_TYPE_DOUBLE = 0x05
	MYSQL_TYPE_NULL = 0x06
	MYSQL_TYPE_TIMESTAMP = 0x07
	MYSQL_TYPE_LONGLONG = 0x08
	MYSQL_TYPE_INT24 = 0x09
	MYSQL_TYPE_DATE = 0x0a
	MYSQL_TYPE_TIME = 0x0b
	MYSQL_TYPE_DATETIME = 0x0c
	MYSQL_TYPE_YEAR = 0x0d
	MYSQL_TYPE_NEWDATE = 0x0e
	MYSQL_TYPE_VARCHAR = 0x0f
	MYSQL_TYPE_BIT = 0x10
	MYSQL_TYPE_TIMESTAMP2 = 0x11
	MYSQL_TYPE_DATETIME2 = 0x12
	MYSQL_TYPE_TIME2 = 0x13
	MYSQL_TYPE_NEWDECIMAL = 0xf6
	MYSQL_TYPE_ENUM = 0xf7
	MYSQL_TYPE_SET = 0xf8
	MYSQL_TYPE_TINY_BLOB = 0xf9
	MYSQL_TYPE_MEDIUM_BLOB = 0xfa
	MYSQL_TYPE_LONG_BLOB = 0xfb
	MYSQL_TYPE_BLOB = 0xfc
	MYSQL_TYPE_VAR_STRING = 0xfd
	MYSQL_TYPE_STRING = 0xfe
	MYSQL_TYPE_GEOMETRY = 0xff
)


type ComQuit struct {
	Command Int1
}

type ComInitDB struct {
	Command Int1
	SchemaName StringEOF	
}

type ComQuery struct {
	Command Int1
	Query StringEOF
}

// See response https://dev.mysql.com/doc/internals/en/com-field-list-response.html
type ComFieldList struct {
	Command Int1
	Table StringNUL
	FieldWildcard StringEOF
}

/*
	The response to ComQuery:
	
	When there are rows:
	- ResultSetHeader packet sent
	- Many ResultSetColun
*/
type ResultSetHeader struct {
	ColumnCount IntVar // If 0, then OK, if 0xff then ERR
}

// Protocol::ColumnDefinition41: https://dev.mysql.com/doc/internals/en/com-query-response.html#column-definition
type ResultSetColumnDefinition struct {
	Catalog StringLenEnc // always 'def'
	Schema StringLenEnc
	Table StringLenEnc
	OrgTable StringLenEnc
	Name StringLenEnc // Name of field
	OrgName StringLenEnc
	FixedLength IntVar // always 0x0c
	CharacterSet Int2
	ColumnLength Int4
	Type Int1 // TODO: List here: https://dev.mysql.com/doc/internals/en/com-query-response.html#column-type
	Flags Int2
	
	// max shown decimal digits
	// 0x00 for integers and static strings
	// 0x1f for dynamic strings, double, float
	// 0x00 to 0x51 for decimals
	Decimals Int1
	Filler Int2 // All 0

	// Optional for COM_FIELD_LIST
	//DefaultValues *StringLenEnc
}

// https://dev.mysql.com/doc/internals/en/com-query-response.html#packet-ProtocolText::ResultsetRow
// A plain list of StringLenEnc values or 0xfb for a null value in a column
type ResultSetRow struct {
	Field1 StringLenEnc
}


func UnmarshalCommand(data []byte) interface{} {
	cmd := uint8(data[0])
	
	switch cmd {
	case COM_QUIT:
		var c ComQuit
		Unmarshal(data, &c)
		return c
	case COM_INIT_DB:
		var c ComInitDB
		Unmarshal(data, &c)
		return c
	case COM_QUERY:
		
		var c ComQuery
		Unmarshal(data, &c)
		fmt.Println("ComQuery", c)
		return c
	
	case COM_FIELD_LIST:
		var c ComFieldList
		Unmarshal(data, &c)
		fmt.Println("ComFieldList", c)
		return c
	default:
		fmt.Println("Got unknown command", cmd, "\n\n")
		panic("Unknown command")
	}

	return nil
}
