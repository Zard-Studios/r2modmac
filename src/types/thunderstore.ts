export interface Community {
    identifier: string;
    name: string;
    discord_url: string;
    wiki_url: string;
    require_package_listing_approval: boolean;
}

export interface PackageVersion {
    name: string;
    full_name: string;
    description: string;
    icon: string;
    version_number: string;
    dependencies: string[];
    download_url: string;
    downloads: number;
    date_created: string;
    website_url: string;
    is_active: boolean;
    uuid4: string;
    file_size: number;
}

export interface Package {
    name: string;
    full_name: string;
    owner: string;
    package_url: string;
    date_created: string;
    date_updated: string;
    uuid4: string;
    rating_score: number;
    is_pinned: boolean;
    is_deprecated: boolean;
    has_nsfw_content: boolean;
    categories: string[];
    versions: PackageVersion[];
}

export interface PackageListing {
    name: string;
    full_name: string;
    owner: string;
    package_url: string;
    date_created: string;
    date_updated: string;
    uuid4: string;
    rating_score: number;
    is_pinned: boolean;
    is_deprecated: boolean;
    has_nsfw_content: boolean;
    categories: string[];
    versions: PackageVersion[];
}
