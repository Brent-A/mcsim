#!/usr/bin/env python3
"""
Script to fetch MeshCore nodes (repeaters, companions, rooms) from the letsmesh.net analyzer API.

Fetches nodes seen in the SEA region in the last 7 days, including:
- Name
- Public key
- Location (lat/lon)
- Recent adverts

Usage:
    python fetch_mesh_nodes.py [--output nodes.json] [--region SEA] [--days 7]
"""

import argparse
import json
import re
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any
from urllib.request import urlopen, Request
from urllib.error import HTTPError, URLError


BASE_URL = "https://api.letsmesh.net/api"
ANALYZER_URL = "https://analyzer.letsmesh.net"

# Known regions from the analyzer site
VALID_REGIONS = [
    "SEA", "PDX", "MFR", "CVO", "EUG",  # Pacific Northwest US
    "YVR", "YYJ", "YCD", "BLI", "KEH", "YSE",  # Canada/BC area
]

# Region groups for broader queries
REGION_GROUPS = {
    "PNW": ["SEA", "PDX", "MFR", "CVO", "EUG", "YVR", "YYJ", "YCD", "BLI", "KEH", "YSE"],
    "SEA": ["SEA"],  # Seattle area
    "PDX": ["PDX"],  # Portland area
    "BC": ["YVR", "YYJ", "YCD", "BLI", "KEH", "YSE"],  # British Columbia
}

# Known seed nodes in the PNW/SEA region (can be used to bootstrap discovery)
# These are public keys of nodes known to be in the SEA area
SEED_NODES_SEA = [
    # Example: VE7RSC North Repeater
    "077E8710C40E04634037CF75CEEC2A2F0F6733BFBB49A720EC44BBE9E6738830",
    # HOWL Repeater  
    "010101C0CDAF7A46D0E4B8B2E1EFD8143F6896E7AC264233686C1BDCFB17205",
]


def fetch_json(url: str, retries: int = 3, delay: float = 1.0) -> Any:
    """Fetch JSON from a URL with retry logic."""
    headers = {
        "User-Agent": "MeshCore-Node-Fetcher/1.0",
        "Accept": "application/json",
    }
    
    for attempt in range(retries):
        try:
            request = Request(url, headers=headers)
            with urlopen(request, timeout=30) as response:
                return json.loads(response.read().decode("utf-8"))
        except HTTPError as e:
            if e.code == 404:
                return None
            if e.code == 429:  # Rate limited
                wait_time = delay * (2 ** attempt)
                print(f"  Rate limited, waiting {wait_time}s...")
                time.sleep(wait_time)
                continue
            raise
        except URLError as e:
            if attempt < retries - 1:
                print(f"  Connection error, retrying in {delay}s: {e}")
                time.sleep(delay)
                continue
            raise
    return None


def fetch_html(url: str) -> str | None:
    """Fetch HTML content from a URL."""
    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    }
    
    try:
        request = Request(url, headers=headers)
        with urlopen(request, timeout=30) as response:
            return response.read().decode("utf-8")
    except Exception as e:
        print(f"  Failed to fetch HTML from {url}: {e}")
        return None


def extract_public_keys_from_html(html: str) -> list[str]:
    """Extract public keys from HTML content using regex."""
    # Public keys are 64-character hex strings
    # They appear in formats like `010101C0...CFB17205` on the page
    # Or as full 64-char hex strings
    
    # Find full 64-char hex strings
    full_keys = re.findall(r'\b([0-9A-Fa-f]{64})\b', html)
    
    # Also look for abbreviated keys and try to find their full versions
    # Format: `XXXXXXXX...YYYYYYYY` where X and Y are 8 chars each
    abbreviated = re.findall(r'`([0-9A-Fa-f]{8})\.\.\.([0-9A-Fa-f]{8})`', html)
    
    # Deduplicate and return
    return list(set(full_keys))


def fetch_nodes_list() -> list[dict]:
    """
    Fetch the list of all known nodes from the API.
    Returns nodes with public_key, name, device_role, regions, first_seen, last_seen, etc.
    """
    url = f"{BASE_URL}/nodes?limit=10000"
    
    try:
        result = fetch_json(url)
        if result and isinstance(result, dict):
            nodes = result.get("nodes", [])
            meta = result.get("meta", {})
            print(f"  API reports {meta.get('total_count', 'unknown')} total nodes, {len(nodes)} returned")
            return nodes
        elif result and isinstance(result, list):
            return result
    except Exception as e:
        print(f"  Failed to fetch nodes list: {e}")
    
    return []


def fetch_node_adverts(public_key: str, limit: int = 500) -> list[dict]:
    """Fetch recent adverts for a specific node."""
    url = f"{BASE_URL}/nodes/{public_key}/adverts?limit={limit}"
    try:
        result = fetch_json(url)
        return result if result else []
    except Exception as e:
        print(f"  Failed to fetch adverts for {public_key[:16]}...: {e}")
        return []


def filter_adverts_by_region_and_time(
    adverts: list[dict],
    region: str,
    since: datetime
) -> list[dict]:
    """Filter adverts to only those in the specified region(s) and time range."""
    # Expand region groups
    target_regions = REGION_GROUPS.get(region, [region])
    
    filtered = []
    for advert in adverts:
        # Check region
        regions = advert.get("regions", [])
        if target_regions and not any(r in regions for r in target_regions):
            continue
        
        # Check time
        heard_at_str = advert.get("heard_at", "")
        if heard_at_str:
            try:
                # Parse ISO timestamp
                heard_at = datetime.fromisoformat(heard_at_str.replace("Z", "+00:00"))
                if heard_at < since:
                    continue
            except ValueError:
                continue
        
        filtered.append(advert)
    
    return filtered


def extract_node_info(adverts: list[dict]) -> dict | None:
    """Extract node information from adverts."""
    if not adverts:
        return None
    
    # Use the most recent advert for basic info
    latest = adverts[0]
    decoded = latest.get("decoded_payload", {})
    
    if not decoded:
        return None
    
    return {
        "name": decoded.get("name", "Unknown"),
        "public_key": decoded.get("public_key", ""),
        "mode": decoded.get("mode", "Unknown"),  # Repeater, Companion, Room Server
        "location": {
            "lat": decoded.get("lat"),
            "lon": decoded.get("lon"),
        },
        "flags": decoded.get("flags"),
        "last_seen": latest.get("heard_at"),
        "regions_seen": list(set(
            region
            for advert in adverts
            for region in advert.get("regions", [])
        )),
    }


def fetch_packets_adverts(region: str | None = None, limit: int = 1000) -> list[dict]:
    """
    Fetch recent packets containing adverts.
    This can be used to discover nodes if a direct nodes list isn't available.
    """
    url = f"{BASE_URL}/packets?payload_type=Advert&limit={limit}"
    if region:
        url += f"&region={region}"
    
    try:
        result = fetch_json(url)
        return result if result else []
    except Exception as e:
        print(f"  Failed to fetch advert packets: {e}")
        return []


def discover_nodes_from_packets(region: str, days: int = 7) -> dict[str, dict]:
    """
    Discover nodes by fetching advert packets from the API.
    This extracts unique nodes from the packet stream.
    """
    since = datetime.now(timezone.utc) - timedelta(days=days)
    discovered_nodes: dict[str, dict] = {}
    
    # Get target regions (expand region groups)
    target_regions = REGION_GROUPS.get(region, [region])
    print(f"Discovering nodes from advert packets in regions: {target_regions}...")
    
    # Fetch advert packets
    packets = fetch_packets_adverts(region=None, limit=10000)  # Get all, filter locally
    
    if packets:
        print(f"Processing {len(packets)} advert packets...")
        for packet in packets:
            # Check if packet is in any target region
            packet_regions = packet.get("regions", [])
            if not any(r in packet_regions for r in target_regions):
                continue
            
            decoded = packet.get("decoded_payload", {})
            if not decoded:
                continue
            
            pk = decoded.get("public_key", "")
            if not pk:
                continue
            
            # Check if this packet is recent enough
            heard_at_str = packet.get("heard_at", "")
            if heard_at_str:
                try:
                    heard_at = datetime.fromisoformat(heard_at_str.replace("Z", "+00:00"))
                    if heard_at < since:
                        continue
                except ValueError:
                    continue
            
            # Add or update node info
            if pk not in discovered_nodes:
                discovered_nodes[pk] = {
                    "public_key": pk,
                    "name": decoded.get("name", "Unknown"),
                    "mode": decoded.get("mode", "Unknown"),
                    "location": {
                        "lat": decoded.get("lat"),
                        "lon": decoded.get("lon"),
                    },
                }
        
        print(f"Discovered {len(discovered_nodes)} unique nodes from packets")
    
    # Also check seed nodes for SEA/PNW
    if region in ("SEA", "PNW") and SEED_NODES_SEA:
        print(f"Adding {len(SEED_NODES_SEA)} seed nodes...")
        for pk in SEED_NODES_SEA:
            if pk not in discovered_nodes:
                discovered_nodes[pk] = {
                    "public_key": pk,
                    "name": "Unknown (seed)",
                    "mode": "Unknown",
                }
    
    return discovered_nodes


def discover_nodes_from_adverts(region: str, days: int = 7) -> dict[str, dict]:
    """
    Discover nodes by fetching from the nodes API.
    Uses multiple strategies to find nodes.
    """
    since = datetime.now(timezone.utc) - timedelta(days=days)
    discovered_nodes: dict[str, dict] = {}
    
    # Get target regions (expand region groups)
    target_regions = REGION_GROUPS.get(region, [region])
    
    # Strategy 1: Get all nodes from API and filter by region
    print("Fetching all nodes from API...")
    nodes_list = fetch_nodes_list()
    
    if nodes_list:
        print(f"Filtering nodes in regions: {target_regions}...")
        for node in nodes_list:
            pk = node.get("public_key", "")
            if not pk:
                continue
            
            # Filter by region
            node_regions = node.get("regions", [])
            if not any(r in node_regions for r in target_regions):
                continue
            
            # Check last_seen time
            last_seen_str = node.get("last_seen", "")
            if last_seen_str:
                try:
                    last_seen = datetime.fromisoformat(last_seen_str.replace("Z", "+00:00"))
                    if last_seen < since:
                        continue
                except ValueError:
                    pass
            
            # Get mode from decoded_payload
            decoded = node.get("decoded_payload", {})
            mode = decoded.get("mode", "Unknown")
            
            discovered_nodes[pk] = {
                "public_key": pk,
                "name": node.get("name", "Unknown"),
                "mode": mode,
                "device_role": node.get("device_role"),
                "regions": node_regions,
                "first_seen": node.get("first_seen"),
                "last_seen": node.get("last_seen"),
                "is_mqtt_connected": node.get("is_mqtt_connected"),
                "location": node.get("location") or {
                    "lat": decoded.get("lat"),
                    "lon": decoded.get("lon"),
                },
            }
        
        print(f"Found {len(discovered_nodes)} nodes in {target_regions}")
    else:
        # Fallback: Try to discover from packet stream
        print("API nodes list unavailable, falling back to packet discovery...")
        discovered_nodes = discover_nodes_from_packets(region, days)
    
    return discovered_nodes


def fetch_all_sea_nodes(
    region: str = "SEA",
    days: int = 7,
    output_file: str | None = None,
    fetch_adverts: bool = True
) -> list[dict]:
    """
    Main function to fetch all nodes in a region.
    
    Strategy:
    1. Get all nodes from the API
    2. Filter to those in the target region(s) seen in the last N days
    3. Optionally fetch detailed adverts for each node
    4. Compile and save node information
    """
    since = datetime.now(timezone.utc) - timedelta(days=days)
    target_regions = REGION_GROUPS.get(region, [region])
    print(f"Fetching nodes seen in {region} ({target_regions}) since {since.isoformat()}")
    print("-" * 60)
    
    # Discover nodes
    discovered = discover_nodes_from_adverts(region, days)
    
    results: list[dict] = []
    processed_keys: set[str] = set()
    
    if discovered:
        total = len(discovered)
        for i, (pk, node_info) in enumerate(discovered.items(), 1):
            if pk in processed_keys:
                continue
            
            processed_keys.add(pk)
            name = node_info.get('name', pk[:16])
            print(f"[{i}/{total}] {name}...", end=" ")
            
            if fetch_adverts:
                # Fetch adverts for this node to get more details
                adverts = fetch_node_adverts(pk, limit=100)
                
                if adverts:
                    # Filter by region and time
                    filtered = filter_adverts_by_region_and_time(adverts, region, since)
                    
                    if filtered:
                        # Update node info from adverts
                        info = extract_node_info(filtered)
                        if info:
                            info["adverts_count"] = len(filtered)
                            info["recent_adverts"] = filtered[:5]  # Keep only 5 most recent
                            # Merge with discovered info
                            info["is_mqtt_connected"] = node_info.get("is_mqtt_connected")
                            info["first_seen"] = node_info.get("first_seen")
                            results.append(info)
                            print(f"{info['mode']} - {len(filtered)} adverts")
                        else:
                            print(f"No valid adverts")
                    else:
                        print(f"No adverts in region/time")
                else:
                    # Use info from nodes list
                    results.append(node_info)
                    print(f"{node_info.get('mode', 'Unknown')} (no adverts fetched)")
                
                # Be nice to the API
                time.sleep(0.3)
            else:
                # Just use the info from the nodes list
                results.append(node_info)
                print(f"{node_info.get('mode', 'Unknown')}")
    
    print("-" * 60)
    print(f"Total nodes found: {len(results)}")
    
    # Categorize
    repeaters = [n for n in results if n.get("mode") == "Repeater"]
    companions = [n for n in results if n.get("mode") == "Companion"]
    rooms = [n for n in results if n.get("mode") in ("Room Server", "Room")]
    other = [n for n in results if n.get("mode") not in ("Repeater", "Companion", "Room Server", "Room")]
    
    print(f"  Repeaters: {len(repeaters)}")
    print(f"  Companions: {len(companions)}")
    print(f"  Room Servers: {len(rooms)}")
    print(f"  Other: {len(other)}")
    
    # Prepare output
    output = {
        "fetched_at": datetime.now(timezone.utc).isoformat(),
        "region": region,
        "target_regions": target_regions,
        "days": days,
        "summary": {
            "total": len(results),
            "repeaters": len(repeaters),
            "companions": len(companions),
            "rooms": len(rooms),
            "other": len(other),
        },
        "nodes": results,
    }
    
    # Save to file if requested
    if output_file:
        output_path = Path(output_file)
        with open(output_path, "w", encoding="utf-8") as f:
            json.dump(output, f, indent=2)
        print(f"\nResults saved to: {output_path}")
    
    return results


def main():
    parser = argparse.ArgumentParser(
        description="Fetch MeshCore nodes from the letsmesh.net analyzer API"
    )
    parser.add_argument(
        "--output", "-o",
        default="sea_nodes.json",
        help="Output JSON file (default: sea_nodes.json)"
    )
    parser.add_argument(
        "--region", "-r",
        default="SEA",
        help=f"Region to filter by. Use 'PNW' for all Pacific Northwest regions. Valid: {', '.join(VALID_REGIONS + list(REGION_GROUPS.keys()))} (default: SEA)"
    )
    parser.add_argument(
        "--days", "-d",
        type=int,
        default=7,
        help="Number of days to look back (default: 7)"
    )
    parser.add_argument(
        "--no-adverts",
        action="store_true",
        help="Skip fetching detailed adverts (faster, but less detail)"
    )
    parser.add_argument(
        "--public-keys", "-k",
        nargs="+",
        help="Specific public keys to fetch (optional)"
    )
    
    args = parser.parse_args()
    
    if args.public_keys:
        # Fetch specific nodes
        results = []
        since = datetime.now(timezone.utc) - timedelta(days=args.days)
        
        for pk in args.public_keys:
            print(f"Fetching {pk[:16]}...")
            adverts = fetch_node_adverts(pk)
            filtered = filter_adverts_by_region_and_time(adverts, args.region, since)
            info = extract_node_info(filtered)
            if info:
                info["adverts_count"] = len(filtered)
                info["recent_adverts"] = filtered[:10]
                results.append(info)
                print(f"  Found: {info['name']} ({info['mode']})")
        
        output = {
            "fetched_at": datetime.now(timezone.utc).isoformat(),
            "region": args.region,
            "days": args.days,
            "nodes": results,
        }
        
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(output, f, indent=2)
        print(f"\nResults saved to: {args.output}")
    else:
        # Fetch all nodes in region
        fetch_all_sea_nodes(
            region=args.region,
            days=args.days,
            output_file=args.output,
            fetch_adverts=not args.no_adverts
        )


if __name__ == "__main__":
    main()
