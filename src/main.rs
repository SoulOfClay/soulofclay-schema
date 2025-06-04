// Import potřebných knihoven
use std::{fs, error::Error, time::Duration};
use chrono::NaiveDate;
use serde::{Deserialize};
use serde_json::json;
use scraper::{Html, Selector};
use reqwest::Client;
use tokio::time::sleep;
use regex::Regex;
use sha2::{Sha256, Digest};

// Definice struktury odpovídající jednomu produktu
#[derive(Debug, Deserialize)]
struct Product {
    name: String,
    image: String,
    description: String,
    sku: String,
    price: f64,
    url: String,
    material: Option<String>,
    volume: Option<String>,
    color: Option<String>,
    availability: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let base_url = "https://www.soulofclay.com";

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; SoulOfClayScraper/1.0)")
        .timeout(Duration::from_secs(10))
        .build()?;

    let body = client.get(base_url).send().await?.text().await?;
    let document = Html::parse_document(&body);

    let product_selector = Selector::parse(".product")?;
    let name_selector = Selector::parse(".name")?;
    let url_selector = Selector::parse("a")?;
    let image_selector = Selector::parse("img")?;
    let price_selector = Selector::parse(".price-final strong")?;

    let detail_desc_selector = Selector::parse(".product-detail-description")?;
    let detail_sku_selector = Selector::parse(".code span")?;
    let param_table_selector = Selector::parse(".parameter-table tr")?;
    let param_name_selector = Selector::parse("th")?;
    let param_value_selector = Selector::parse("td")?;

    let price_regex = Regex::new(r"[^\d,]").unwrap();
    let tag_regex = Regex::new(r"<[^>]*>").unwrap();

    let mut products = Vec::new();

    for element in document.select(&product_selector) {
        let raw_name = element.select(&name_selector).next().map(|n| n.inner_html()).unwrap_or_default();
        let name = tag_regex.replace_all(&raw_name, "").trim().to_string();

        let relative_url = element.select(&url_selector).next().and_then(|a| a.value().attr("href")).unwrap_or("/");
        let url = format!("{}{}", base_url, relative_url);

        let image = element.select(&image_selector).next().and_then(|img| img.value().attr("data-src")).unwrap_or("").replace("\n", "");

        let price_text = element.select(&price_selector).next().map(|p| p.inner_html()).unwrap_or("0".to_string());
        let cleaned_price = price_regex.replace_all(&price_text, "").replace(",", ".");

        let price = match cleaned_price.parse::<f64>() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("⚠️ Chyba při parsování ceny: '{}'", price_text);
                0.0
            }
        };

        sleep(Duration::from_millis(500)).await;

        let mut detail_body = None;
        for _ in 0..3 {
            match client.get(&url).send().await {
                Ok(resp) => match resp.text().await {
                    Ok(text) => {
                        detail_body = Some(text);
                        break;
                    }
                    Err(err) => {
                        eprintln!("❌ Chyba při čtení textu z detailu {}: {}", url, err);
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                },
                Err(err) => {
                    eprintln!("❌ Chyba při HTTP požadavku na {}: {}", url, err);
                    sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }

        let detail_body = match detail_body {
            Some(text) => text,
            None => {
                eprintln!("❌ Nepodařilo se načíst detail produktu po 3 pokusech: {}", url);
                continue;
            }
        };

        let detail_doc = Html::parse_document(&detail_body);

        let description = detail_doc
            .select(&detail_desc_selector)
            .next()
            .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_else(|| "Ručně vyráběná keramika Soul of Clay".to_string());

        let sku = detail_doc
            .select(&detail_sku_selector)
            .next()
            .map(|e| e.text().collect::<Vec<_>>().join("").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                let mut hasher = Sha256::new();
                hasher.update(&name);
                let hash = format!("{:x}", hasher.finalize());
                format!("sku-{}", &hash[..8])
            });

        let mut material = None;
        let mut volume = None;
        let mut color = None;

        for row in detail_doc.select(&param_table_selector) {
            let name = row.select(&param_name_selector).next().map(|n| n.text().collect::<Vec<_>>().join(" ").trim().to_string()).unwrap_or_default();
            let value = row.select(&param_value_selector).next().map(|v| v.text().collect::<Vec<_>>().join(" ").trim().to_string()).unwrap_or_default();

            match name.to_lowercase().as_str() {
                "materiál" => material = Some(value),
                "objem" => volume = Some(value),
                "barva" => color = Some(value),
                _ => {}
            }
        }

        products.push(Product {
            name,
            image,
            description,
            sku,
            price,
            url,
            material,
            volume,
            color,
            availability: "https://schema.org/InStock".to_string(),
        });
    }

    let brand = json!({
        "@type": "Organization",
        "name": "Soul of Clay",
        "url": base_url,
        "logo": base_url,
        "description": "Autorské kameninové nádobí od Evy Slabé, vyráběné malosériově s vlastnoručně míchanými glazurami.",
        "founder": "Eva Slabá",
        "foundingLocation": "Praha, Česká republika",
        "sameAs": ["https://www.facebook.com/hlavahlinena"],
        "address": {
            "@type": "PostalAddress",
            "streetAddress": "Doubravice 1.díl 29",
            "postalCode": "257 22",
            "addressLocality": "Přestavlky u Čerčan",
            "addressCountry": "CZ"
        },
        "contactPoint": {
            "@type": "ContactPoint",
            "contactType": "Customer Service",
            "email": "eva@soulofclay.com"
        },
        "subjectOf": [
            {
                "@type": "BlogPosting",
                "headline": "Uhlíková stopa hrnku",
                "mainEntityOfPage": "https://www.soulofclay.com/blog/uhlikova-stopa-hrnku/"
            }
        ]
    });

    let valid_until = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap().to_string();

    let mut graph = vec![brand.clone()];

    for p in &products {
        let mut product_json = json!({
            "@type": "Product",
            "name": p.name,
            "image": p.image,
            "description": p.description,
            "sku": p.sku,
            "brand": { "@type": "Brand", "name": "Soul of Clay" },
            "offers": {
                "@type": "Offer",
                "priceCurrency": "CZK",
                "price": p.price,
                "priceValidUntil": valid_until,
                "url": p.url,
                "availability": p.availability,
                "identifierExists": false
            },
            "subjectOf": {
                "@id": "https://www.soulofclay.com/blog/uhlikova-stopa-hrnku/"
            }
        });

        if let Some(ref m) = p.material {
            product_json["material"] = json!(m);
        }
        if let Some(ref v) = p.volume {
            product_json["volume"] = json!(v);
        }
        if let Some(ref c) = p.color {
            product_json["color"] = json!(c);
        }

        graph.push(product_json);
    }

    let full_jsonld = json!({
        "@context": "https://schema.org",
        "@graph": graph
    });

    fs::write("products.json", serde_json::to_string_pretty(&full_jsonld)?)?;
    fs::write("output.html", format!(
        "<script type=\"application/ld+json\">\n{}\n</script>",
        serde_json::to_string_pretty(&full_jsonld)?
    ))?;
    fs::write("brand.json", serde_json::to_string_pretty(&brand)?)?;
    fs::write("index.html", r#"<!DOCTYPE html>
<html lang="cs">
<head>
  <meta charset="UTF-8">
  <title>Soul of Clay - Schema</title>
</head>
<body>
  <h1>📦 Soul of Clay – JSON-LD výstup</h1>
  <p><a href="products.json">products.json</a></p>
  <p><a href="brand.json">brand.json</a></p>
</body>
</html>"#)?;

    println!("✅ Načteny produkty a článek propojen, vygenerováno s plným kontextem a index.html přidán.");
    Ok(())
}
