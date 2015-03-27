initSidebarItems({"struct":[["EventStream","`EventStream` wraps a stream and will loop, reading lines and processing them with `Handler`s, which are allowed to write back to the stream and install additional `Handler`s."],["Response","Optionally respond with a message, then take the given next `Action`."]],"type":[["Handler","A `Handler` examines a line of input read from a stream, and produces an optional `Response` to send, an additional `Handler` to install (perhaps to wait for a response), and a next `Action` to take after processing the line."]],"enum":[["Action","What should we do next?"],["HandlerAction",""]]});