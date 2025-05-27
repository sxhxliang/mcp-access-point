from fastapi import FastAPI, HTTPException, Depends, File, UploadFile, Form, Header, Query
from fastapi.security import HTTPBearer, OAuth2PasswordBearer
from pydantic import BaseModel, Field
from typing import List, Optional, Dict
from enum import Enum
from datetime import datetime
import uvicorn

app = FastAPI(
    title="Swagger Petstore",
    description="This is a sample server Petstore server. You can find out more about Swagger at [http://swagger.io](http://swagger.io) or on [irc.freenode.net, #swagger](http://swagger.io/irc/). For this sample, you can use the api key `special-key` to test the authorization filters.",
    version="1.0.7",
    terms_of_service="http://swagger.io/terms/",
    contact={
        "email": "apiteam@swagger.io"
    },
    license_info={
        "name": "Apache 2.0",
        "url": "http://www.apache.org/licenses/LICENSE-2.0.html",
    },
)

# Security schemes
security = HTTPBearer()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

# Enums
class PetStatus(str, Enum):
    available = "available"
    pending = "pending"
    sold = "sold"

class OrderStatus(str, Enum):
    placed = "placed"
    approved = "approved"
    delivered = "delivered"

# Models
class Category(BaseModel):
    id: Optional[int] = Field(None, format="int64")
    name: Optional[str] = None

class Tag(BaseModel):
    id: Optional[int] = Field(None, format="int64")
    name: Optional[str] = None

class Pet(BaseModel):
    id: Optional[int] = Field(None, format="int64")
    category: Optional[Category] = None
    name: str = Field(..., example="doggie")
    photoUrls: List[str] = Field(..., description="Pet photo URLs")
    tags: Optional[List[Tag]] = None
    status: Optional[PetStatus] = Field(None, description="pet status in the store")

class ApiResponse(BaseModel):
    code: Optional[int] = Field(None, format="int32")
    type: Optional[str] = None
    message: Optional[str] = None

class Order(BaseModel):
    id: Optional[int] = Field(None, format="int64")
    petId: Optional[int] = Field(None, format="int64")
    quantity: Optional[int] = Field(None, format="int32")
    shipDate: Optional[datetime] = None
    status: Optional[OrderStatus] = Field(None, description="Order Status")
    complete: Optional[bool] = None

class User(BaseModel):
    id: Optional[int] = Field(None, format="int64")
    username: Optional[str] = None
    firstName: Optional[str] = None
    lastName: Optional[str] = None
    email: Optional[str] = None
    password: Optional[str] = None
    phone: Optional[str] = None
    userStatus: Optional[int] = Field(None, format="int32", description="User Status")

# In-memory storage (for demo purposes)
pets_db: Dict[int, Pet] = {}
orders_db: Dict[int, Order] = {}
users_db: Dict[str, User] = {}
inventory_db: Dict[str, int] = {"available": 5, "pending": 3, "sold": 2}

# Helper functions
def get_next_pet_id():
    return max(pets_db.keys()) + 1 if pets_db else 1

def get_next_order_id():
    return max(orders_db.keys()) + 1 if orders_db else 1

# Pet endpoints
@app.post("/pet/{petId}/uploadImage", tags=["pet"], response_model=ApiResponse)
async def upload_file(
    petId: int,
    additionalMetadata: Optional[str] = Form(None, description="Additional data to pass to server"),
    file: Optional[UploadFile] = File(None, description="file to upload")
):
    """uploads an image"""
    if petId not in pets_db:
        raise HTTPException(status_code=404, detail="Pet not found")
    
    return ApiResponse(code=200, type="success", message="Image uploaded successfully")

@app.post("/pet", tags=["pet"])
async def add_pet(pet: Pet):
    """Add a new pet to the store"""
    if not pet.name or not pet.photoUrls:
        raise HTTPException(status_code=405, detail="Invalid input")
    
    pet_id = get_next_pet_id()
    pet.id = pet_id
    pets_db[pet_id] = pet
    return pet

@app.put("/pet", tags=["pet"])
async def update_pet(pet: Pet):
    """Update an existing pet"""
    if not pet.id:
        raise HTTPException(status_code=400, detail="Invalid ID supplied")
    
    if pet.id not in pets_db:
        raise HTTPException(status_code=404, detail="Pet not found")
    
    pets_db[pet.id] = pet
    return pet

@app.get("/pet/findByStatus", tags=["pet"], response_model=List[Pet])
async def find_pets_by_status(status: List[PetStatus] = Query(..., description="Status values that need to be considered for filter")):
    """Finds Pets by status"""
    result = []
    for pet in pets_db.values():
        if pet.status in status:
            result.append(pet)
    return result

@app.get("/pet/findByTags", tags=["pet"], response_model=List[Pet], deprecated=True)
async def find_pets_by_tags(tags: List[str] = Query(..., description="Tags to filter by")):
    """Finds Pets by tags"""
    result = []
    for pet in pets_db.values():
        if pet.tags:
            pet_tag_names = [tag.name for tag in pet.tags if tag.name]
            if any(tag in pet_tag_names for tag in tags):
                result.append(pet)
    return result

@app.get("/pet/{petId}", tags=["pet"], response_model=Pet)
async def get_pet_by_id(petId: int):
    """Find pet by ID"""
    if petId not in pets_db:
        raise HTTPException(status_code=404, detail="Pet not found")
    return pets_db[petId]

@app.post("/pet/{petId}", tags=["pet"])
async def update_pet_with_form(
    petId: int,
    name: Optional[str] = Form(None, description="Updated name of the pet"),
    status: Optional[str] = Form(None, description="Updated status of the pet")
):
    """Updates a pet in the store with form data"""
    if petId not in pets_db:
        raise HTTPException(status_code=405, detail="Invalid input")
    
    pet = pets_db[petId]
    if name:
        pet.name = name
    if status:
        pet.status = status
    
    return {"message": "Pet updated successfully"}

@app.delete("/pet/{petId}", tags=["pet"])
async def delete_pet(
    petId: int,
    api_key: Optional[str] = Header(None)
):
    """Deletes a pet"""
    if petId not in pets_db:
        raise HTTPException(status_code=404, detail="Pet not found")
    
    del pets_db[petId]
    return {"message": "Pet deleted successfully"}

# Store endpoints
@app.get("/store/inventory", tags=["store"], response_model=Dict[str, int])
async def get_inventory():
    """Returns pet inventories by status"""
    return inventory_db

@app.post("/store/order", tags=["store"], response_model=Order)
async def place_order(order: Order):
    """Place an order for a pet"""
    if not order.petId or not order.quantity:
        raise HTTPException(status_code=400, detail="Invalid Order")
    
    order_id = get_next_order_id()
    order.id = order_id
    orders_db[order_id] = order
    return order

@app.get("/store/order/{orderId}", tags=["store"], response_model=Order)
async def get_order_by_id(orderId: int):
    """Find purchase order by ID"""
    if orderId < 1 or orderId > 10:
        raise HTTPException(status_code=400, detail="Invalid ID supplied")
    if orderId not in orders_db:
        raise HTTPException(status_code=404, detail="Order not found")
    return orders_db[orderId]

@app.delete("/store/order/{orderId}", tags=["store"])
async def delete_order(orderId: int):
    """Delete purchase order by ID"""
    if orderId < 1:
        raise HTTPException(status_code=400, detail="Invalid ID supplied")
    if orderId not in orders_db:
        raise HTTPException(status_code=404, detail="Order not found")
    
    del orders_db[orderId]
    return {"message": "Order deleted successfully"}

# User endpoints
@app.post("/user/createWithList", tags=["user"])
async def create_users_with_list_input(users: List[User]):
    """Creates list of users with given input array"""
    for user in users:
        if user.username:
            users_db[user.username] = user
    return {"message": "Users created successfully"}

@app.get("/user/{username}", tags=["user"], response_model=User)
async def get_user_by_name(username: str):
    """Get user by user name"""
    if username not in users_db:
        raise HTTPException(status_code=404, detail="User not found")
    return users_db[username]

@app.put("/user/{username}", tags=["user"])
async def update_user(username: str, user: User):
    """Updated user"""
    if username not in users_db:
        raise HTTPException(status_code=404, detail="User not found")
    
    users_db[username] = user
    return {"message": "User updated successfully"}

@app.delete("/user/{username}", tags=["user"])
async def delete_user(username: str):
    """Delete user"""
    if username not in users_db:
        raise HTTPException(status_code=404, detail="User not found")
    
    del users_db[username]
    return {"message": "User deleted successfully"}

@app.get("/user/login", tags=["user"], response_model=str)
async def login_user(
    username: str = Query(..., description="The user name for login"),
    password: str = Query(..., description="The password for login in clear text")
):
    """Logs user into the system"""
    # Simple validation for demo
    if username == "user1" and password == "password":
        return "logged_in_session_token"
    else:
        raise HTTPException(status_code=400, detail="Invalid username/password supplied")

@app.get("/user/logout", tags=["user"])
async def logout_user():
    """Logs out current logged in user session"""
    return {"message": "User logged out successfully"}

@app.post("/user/createWithArray", tags=["user"])
async def create_users_with_array_input(users: List[User]):
    """Creates list of users with given input array"""
    for user in users:
        if user.username:
            users_db[user.username] = user
    return {"message": "Users created successfully"}

@app.post("/user", tags=["user"])
async def create_user(user: User):
    """Create user"""
    if user.username:
        users_db[user.username] = user
    return {"message": "User created successfully"}

# Add some sample data for testing
def init_sample_data():
    # Sample pets
    sample_pet = Pet(
        id=1,
        name="Buddy",
        photoUrls=["http://example.com/photo1.jpg"],
        status=PetStatus.available,
        category=Category(id=1, name="Dogs"),
        tags=[Tag(id=1, name="friendly")]
    )
    pets_db[1] = sample_pet
    
    # Sample user
    sample_user = User(
        id=1,
        username="user1",
        firstName="John",
        lastName="Doe",
        email="john@example.com",
        password="password",
        phone="123-456-7890",
        userStatus=1
    )
    users_db["user1"] = sample_user
    
    # Sample order
    sample_order = Order(
        id=1,
        petId=1,
        quantity=1,
        shipDate=datetime.now(),
        status=OrderStatus.placed,
        complete=False
    )
    orders_db[1] = sample_order

# Initialize sample data
init_sample_data()

if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=8090)