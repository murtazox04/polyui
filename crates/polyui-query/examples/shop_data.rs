use datafusion::arrow::array::{ArrayRef, Date32Array, Float64Array, Int64Array, StringArray};
use datafusion::arrow::datatypes::{DataType, Field, Schema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::parquet::arrow::ArrowWriter;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

const SEED: u64 = 0x5eed_2026_1124;
const FIRST_DAY: i32 = 20454;
const DAYS: i32 = 327;
const CUSTOMERS: usize = 240;

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit.max(1)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

struct Product {
    name: &'static str,
    category: &'static str,
    price: i64,
    supplier_cost: i64,
    weight: u64,
}

const PRODUCTS: [Product; 12] = [
    Product {
        name: "Espresso beans 1 kg",
        category: "Coffee",
        price: 285_000,
        supplier_cost: 170_000,
        weight: 9,
    },
    Product {
        name: "Filter coffee 500 g",
        category: "Coffee",
        price: 145_000,
        supplier_cost: 82_000,
        weight: 8,
    },
    Product {
        name: "Decaf beans 500 g",
        category: "Coffee",
        price: 160_000,
        supplier_cost: 95_000,
        weight: 3,
    },
    Product {
        name: "Green tea 100 g",
        category: "Tea",
        price: 65_000,
        supplier_cost: 31_000,
        weight: 7,
    },
    Product {
        name: "Black tea 250 g",
        category: "Tea",
        price: 72_000,
        supplier_cost: 35_000,
        weight: 6,
    },
    Product {
        name: "Herbal tea set",
        category: "Tea",
        price: 118_000,
        supplier_cost: 60_000,
        weight: 3,
    },
    Product {
        name: "Pour-over kettle",
        category: "Equipment",
        price: 540_000,
        supplier_cost: 330_000,
        weight: 2,
    },
    Product {
        name: "Hand grinder",
        category: "Equipment",
        price: 690_000,
        supplier_cost: 410_000,
        weight: 2,
    },
    Product {
        name: "French press",
        category: "Equipment",
        price: 310_000,
        supplier_cost: 175_000,
        weight: 3,
    },
    Product {
        name: "Paper filters x100",
        category: "Accessories",
        price: 38_000,
        supplier_cost: 14_000,
        weight: 6,
    },
    Product {
        name: "Ceramic mug",
        category: "Accessories",
        price: 95_000,
        supplier_cost: 41_000,
        weight: 4,
    },
    Product {
        name: "Travel tumbler",
        category: "Accessories",
        price: 175_000,
        supplier_cost: 88_000,
        weight: 3,
    },
];

const CITIES: [(&str, u64); 6] = [
    ("Tashkent", 10),
    ("Samarkand", 4),
    ("Bukhara", 3),
    ("Namangan", 3),
    ("Andijan", 2),
    ("Fergana", 2),
];

const FIRST_NAMES: [&str; 16] = [
    "Aziza", "Bekzod", "Dilnoza", "Farrukh", "Gulnora", "Jasur", "Kamola", "Laziz", "Madina",
    "Nodir", "Otabek", "Sevara", "Timur", "Umida", "Zafar", "Shahnoza",
];

const LAST_NAMES: [&str; 12] = [
    "Karimov",
    "Rakhimova",
    "Tursunov",
    "Yusupova",
    "Aliyev",
    "Saidova",
    "Ergashev",
    "Nazarova",
    "Usmonov",
    "Kadirova",
    "Mirzaev",
    "Islomova",
];

const CHANNELS: [(&str, u64); 3] = [("web", 6), ("app", 3), ("store", 2)];

fn weighted<'a>(random: &mut Random, items: &'a [(&'a str, u64)]) -> &'a str {
    let total: u64 = items.iter().map(|(_, weight)| weight).sum();
    let mut roll = random.below(total);
    for (name, weight) in items {
        if roll < *weight {
            return name;
        }
        roll -= weight;
    }
    items[0].0
}

fn product_index(random: &mut Random) -> usize {
    let total: u64 = PRODUCTS.iter().map(|product| product.weight).sum();
    let mut roll = random.below(total);
    for (index, product) in PRODUCTS.iter().enumerate() {
        if roll < product.weight {
            return index;
        }
        roll -= product.weight;
    }
    0
}

fn date_text(days: i32) -> String {
    let civil = days as i64 + 719_468;
    let era = civil.div_euclid(146_097);
    let day_of_era = civil - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn daily_orders(random: &mut Random, day: i32) -> u64 {
    let month = date_text(day)[5..7].parse::<u64>().unwrap_or(1);
    let season = match month {
        11 => 9,
        10 | 12 => 7,
        1 | 2 => 6,
        6..=8 => 4,
        _ => 5,
    };
    let weekday = (day + 4).rem_euclid(7);
    let weekend = if weekday == 0 || weekday == 6 { 2 } else { 0 };
    random.below(season + weekend) + 1
}

fn products_csv() -> String {
    let mut text = String::from("product_id,name,category,price,supplier_cost\n");
    for (index, product) in PRODUCTS.iter().enumerate() {
        let _ = writeln!(
            text,
            "{},{},{},{},{}",
            index + 1,
            product.name,
            product.category,
            product.price,
            product.supplier_cost
        );
    }
    text
}

fn customers_csv(random: &mut Random) -> String {
    let mut text = String::from("customer_id,name,city,signup_date,email,phone\n");
    for id in 1..=CUSTOMERS {
        let first = random.pick(&FIRST_NAMES);
        let last = random.pick(&LAST_NAMES);
        let city = weighted(random, &CITIES);
        let signup = FIRST_DAY - 400 + random.below(700) as i32;
        let _ = writeln!(
            text,
            "{id},{first} {last},{city},{},{}.{}{id}@example.uz,+99890{:07}",
            date_text(signup),
            first.to_lowercase(),
            last.to_lowercase(),
            random.below(10_000_000)
        );
    }
    text
}

fn orders_batch(random: &mut Random) -> Result<RecordBatch, String> {
    let mut order_id = Vec::new();
    let mut order_date = Vec::new();
    let mut customer_id = Vec::new();
    let mut product_id = Vec::new();
    let mut quantity = Vec::new();
    let mut unit_price = Vec::new();
    let mut discount = Vec::new();
    let mut channel = Vec::new();
    let mut status = Vec::new();
    for day in FIRST_DAY..FIRST_DAY + DAYS {
        for _ in 0..daily_orders(random, day) {
            let index = product_index(random);
            order_id.push(order_id.len() as i64 + 1001);
            order_date.push(day);
            customer_id.push(random.below(CUSTOMERS as u64) as i64 + 1);
            product_id.push(index as i64 + 1);
            quantity.push(1 + (random.unit() * random.unit() * 4.0) as i64);
            unit_price.push(PRODUCTS[index].price);
            let off = if random.below(5) == 0 {
                [0.05, 0.1, 0.15][random.below(3) as usize]
            } else {
                0.0
            };
            discount.push(off);
            channel.push(weighted(random, &CHANNELS));
            status.push(if random.below(25) == 0 {
                "refunded"
            } else {
                "paid"
            });
        }
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("order_id", DataType::Int64, false),
        Field::new("order_date", DataType::Date32, false),
        Field::new("customer_id", DataType::Int64, false),
        Field::new("product_id", DataType::Int64, false),
        Field::new("quantity", DataType::Int64, false),
        Field::new("unit_price", DataType::Int64, false),
        Field::new("discount", DataType::Float64, false),
        Field::new("channel", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, false),
    ]));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(order_id)),
        Arc::new(Date32Array::from(order_date)),
        Arc::new(Int64Array::from(customer_id)),
        Arc::new(Int64Array::from(product_id)),
        Arc::new(Int64Array::from(quantity)),
        Arc::new(Int64Array::from(unit_price)),
        Arc::new(Float64Array::from(discount)),
        Arc::new(StringArray::from(channel)),
        Arc::new(StringArray::from(status)),
    ];
    RecordBatch::try_new(schema, columns).map_err(|error| error.to_string())
}

fn write_parquet(path: &Path, batch: &RecordBatch) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    let mut writer =
        ArrowWriter::try_new(file, batch.schema(), None).map_err(|error| error.to_string())?;
    writer.write(batch).map_err(|error| error.to_string())?;
    writer.close().map_err(|error| error.to_string())?;
    Ok(())
}

fn generate(dir: &Path) -> Result<usize, String> {
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let mut random = Random(SEED);
    fs::write(dir.join("products.csv"), products_csv()).map_err(|error| error.to_string())?;
    fs::write(dir.join("customers.csv"), customers_csv(&mut random))
        .map_err(|error| error.to_string())?;
    let orders = orders_batch(&mut random)?;
    write_parquet(&dir.join("orders.parquet"), &orders)?;
    Ok(orders.num_rows())
}

fn main() -> ExitCode {
    let dir = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("examples/data/shop"), PathBuf::from);
    match generate(&dir) {
        Ok(orders) => {
            println!(
                "wrote products.csv, customers.csv and orders.parquet ({orders} orders) to {}",
                dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not write the shop data: {error}");
            ExitCode::FAILURE
        }
    }
}
