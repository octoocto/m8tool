class_name FileOperation

##
# Returns true if a file needs to have this operation performed on it.
##
func is_operation_needed() -> bool:
    return false

##
# A short description of what this operation is going to do.
##
func get_description() -> String:
    return "unknown operation"

##
# Perform the operation.
##
func perform() -> void:
    return
